#define LOG_TAG "SosCoreHost"

#include <android-base/properties.h>
#include <android/native_window.h>
#include <arpa/inet.h>
#include <binder/IServiceManager.h>
#include <binder/ProcessState.h>
#include <dlfcn.h>
#include <fcntl.h>
#include <gui/ISurfaceComposer.h>
#include <gui/Surface.h>
#include <gui/SurfaceComposerClient.h>
#include <linux/input.h>
#include <log/log.h>
#include <poll.h>
#include <signal.h>
#include <sys/ioctl.h>
#include <sys/socket.h>
#include <sys/un.h>
#include <sys/wait.h>
#include <ui/DisplayMode.h>
#include <ui/GraphicTypes.h>
#include <ui/PixelFormat.h>
#include <unistd.h>
#include <utils/Errors.h>
#include <utils/String8.h>
#include <utils/StrongPointer.h>

#include <algorithm>
#include <cerrno>
#include <cstddef>
#include <cstdint>
#include <cstring>
#include <string>
#include <vector>

namespace {

using android::IBinder;
using android::IServiceManager;
using android::ISurfaceComposerClient;
using android::PIXEL_FORMAT_RGBA_8888;
using android::ProcessState;
using android::sp;
using android::status_t;
using android::String16;
using android::String8;
using android::Surface;
using android::SurfaceComposerClient;
using android::SurfaceControl;
using android::ui::DisplayMode;

constexpr int kServiceWaitMicros = 100'000;
constexpr int kExitRecoveryRequested = 100;
constexpr int kExitAndroidUserLocked = 101;
constexpr int kExitCompatHandoff = 102;
constexpr int32_t kCoreLayer = 0x40000002;
constexpr int32_t kRecoveryLayer = 0x40000001;
constexpr int32_t kLockLayer = 0x40000003;
constexpr int kBridgeMagic = 0x534f5331;
constexpr int kBridgeStatus = 1;
constexpr int kBridgeVerifyPin = 2;
constexpr int kBridgeStartHome = 3;
constexpr int kBridgeOk = 1;
constexpr int kBridgeRejected = 2;
constexpr int kBridgeRetry = 3;
constexpr int kCredentialNone = -1;
constexpr int kCredentialPin = 3;
constexpr char kBridgeSocket[] = "sos_framework_bridge";
constexpr int kControlMagic = 0x534f5332;
constexpr int kControlLock = 1;
constexpr int kControlHomeFailed = 2;
constexpr uid_t kSystemUid = 1000;
constexpr char kControlSocket[] = "sos_native_shell_control";
constexpr char kExperienceLibrary[] =
    "/system_ext/lib64/libsos_core_experience.so";
constexpr char kCoreDataDirectory[] = "/data/misc/sos/core";
constexpr char kHostExecutable[] = "/system_ext/bin/sos-core-host";

using CoreMain = int (*)(ANativeWindow *, int32_t, const char *);
using CoreProviderAcceptanceProbe = int (*)(const char *);

struct NativeSurface {
  sp<SurfaceComposerClient> client;
  sp<SurfaceControl> control;
  sp<Surface> surface;
  int32_t width = 0;
  int32_t height = 0;
};

int runPreUnlockGate();

void waitForSurfaceFlinger() {
  const sp<IServiceManager> services = android::defaultServiceManager();
  const String16 name("SurfaceFlinger");
  while (services->checkService(name) == nullptr) {
    usleep(kServiceWaitMicros);
  }
}

bool createSurface(const char *name, int32_t layer, NativeSurface *result) {
  const sp<ProcessState> process = ProcessState::self();
  process->startThreadPool();
  waitForSurfaceFlinger();

  const auto displayIds = SurfaceComposerClient::getPhysicalDisplayIds();
  if (displayIds.empty()) {
    ALOGE("no physical display is available");
    return false;
  }
  const sp<IBinder> displayToken =
      SurfaceComposerClient::getPhysicalDisplayToken(displayIds.front());
  if (displayToken == nullptr) {
    ALOGE("the primary display has no SurfaceFlinger token");
    return false;
  }

  DisplayMode mode;
  const status_t modeStatus =
      SurfaceComposerClient::getActiveDisplayMode(displayToken, &mode);
  if (modeStatus != android::NO_ERROR) {
    ALOGE("cannot read the active display mode: %d", modeStatus);
    return false;
  }

  result->client = sp<SurfaceComposerClient>::make();
  if (result->client->initCheck() != android::NO_ERROR) {
    ALOGE("cannot connect to SurfaceFlinger");
    return false;
  }
  result->width = mode.resolution.getWidth();
  result->height = mode.resolution.getHeight();
  result->control = result->client->createSurface(
      String8(name), result->width, result->height, PIXEL_FORMAT_RGBA_8888,
      ISurfaceComposerClient::eOpaque);
  if (result->control == nullptr || !result->control->isValid()) {
    ALOGE("cannot create native surface %s", name);
    return false;
  }
  result->surface = result->control->getSurface();
  if (result->surface == nullptr) {
    ALOGE("SurfaceFlinger did not expose an ANativeWindow");
    return false;
  }
  SurfaceComposerClient::Transaction{}
      .setLayer(result->control, layer)
      .show(result->control)
      .apply();
  return true;
}

int runExperience() {
  if (!android::base::GetBoolProperty("sys.user.0.ce_available", false)) {
    const int gate = runPreUnlockGate();
    if (gate != 0)
      return gate;
  }
  const std::string stage =
      android::base::GetProperty("ro.sos.core.stage", "shadow");
  if (stage == "compat") {
    // Native Compat owns the pre-unlock surface and raw input, then gets out
    // of WindowManager's way. The platform-signed GPUI HOME is the only SOS
    // Activity host; explicitly selected non-system Activities may render
    // beneath its persistent trusted controls.
    ALOGI("native_compat_handoff target=sos-home android_system_ui=false");
    return kExitCompatHandoff;
  }
  NativeSurface nativeSurface;
  if (!createSurface("SOS Core Experience", kCoreLayer, &nativeSurface)) {
    return 1;
  }

  void *library = dlopen(kExperienceLibrary, RTLD_NOW | RTLD_LOCAL);
  if (library == nullptr) {
    ALOGE("cannot load %s: %s", kExperienceLibrary, dlerror());
    return 1;
  }
  dlerror();
  auto coreMain = reinterpret_cast<CoreMain>(dlsym(library, "sos_core_main"));
  if (const char *error = dlerror(); error != nullptr || coreMain == nullptr) {
    ALOGE("cannot resolve sos_core_main: %s",
          error == nullptr ? "missing" : error);
    return 1;
  }

  const int32_t density =
      android::base::GetIntProperty("ro.sf.lcd_density", 450);
  ALOGI("native_gpui_start width=%d height=%d density=%d ui_owner=%s",
        nativeSurface.width, nativeSurface.height, density,
        android::base::GetProperty("ro.sos.ui_owner", "unknown").c_str());
  const int result =
      coreMain(nativeSurface.surface.get(), density, kCoreDataDirectory);
  ALOGI("native_gpui_stopped status=%d", result);
  SurfaceComposerClient::Transaction{}.hide(nativeSurface.control).apply();
  return result;
}

uint32_t recoveryColor(int x, int y, int width, int height) {
  const bool header = y < height / 5;
  const bool retry = y > height / 2 && y < (height * 2) / 3 && x > width / 12 &&
                     x < (width * 11) / 12;
  const bool android = y > (height * 3) / 4 && y < (height * 11) / 12 &&
                       x > width / 12 && x < (width * 11) / 12;
  if (header)
    return 0xff17211d;
  if (retry)
    return 0xff245b43;
  if (android)
    return 0xff49312c;
  return 0xff0c1411;
}

const uint8_t *glyph(char character) {
  static constexpr uint8_t kSpace[7] = {0, 0, 0, 0, 0, 0, 0};
  static constexpr uint8_t k0[7] = {14, 17, 19, 21, 25, 17, 14};
  static constexpr uint8_t k1[7] = {4, 12, 4, 4, 4, 4, 14};
  static constexpr uint8_t k2[7] = {14, 17, 1, 2, 4, 8, 31};
  static constexpr uint8_t k3[7] = {30, 1, 1, 14, 1, 1, 30};
  static constexpr uint8_t k4[7] = {2, 6, 10, 18, 31, 2, 2};
  static constexpr uint8_t k5[7] = {31, 16, 16, 30, 1, 1, 30};
  static constexpr uint8_t k6[7] = {14, 16, 16, 30, 17, 17, 14};
  static constexpr uint8_t k7[7] = {31, 1, 2, 4, 8, 8, 8};
  static constexpr uint8_t k8[7] = {14, 17, 17, 14, 17, 17, 14};
  static constexpr uint8_t k9[7] = {14, 17, 17, 15, 1, 1, 14};
  static constexpr uint8_t kA[7] = {14, 17, 17, 31, 17, 17, 17};
  static constexpr uint8_t kB[7] = {30, 17, 17, 30, 17, 17, 30};
  static constexpr uint8_t kC[7] = {14, 17, 16, 16, 16, 17, 14};
  static constexpr uint8_t kD[7] = {30, 17, 17, 17, 17, 17, 30};
  static constexpr uint8_t kE[7] = {31, 16, 16, 30, 16, 16, 31};
  static constexpr uint8_t kF[7] = {31, 16, 16, 30, 16, 16, 16};
  static constexpr uint8_t kG[7] = {14, 17, 16, 23, 17, 17, 15};
  static constexpr uint8_t kH[7] = {17, 17, 17, 31, 17, 17, 17};
  static constexpr uint8_t kI[7] = {31, 4, 4, 4, 4, 4, 31};
  static constexpr uint8_t kJ[7] = {7, 2, 2, 2, 18, 18, 12};
  static constexpr uint8_t kK[7] = {17, 18, 20, 24, 20, 18, 17};
  static constexpr uint8_t kL[7] = {16, 16, 16, 16, 16, 16, 31};
  static constexpr uint8_t kM[7] = {17, 27, 21, 21, 17, 17, 17};
  static constexpr uint8_t kN[7] = {17, 25, 21, 19, 17, 17, 17};
  static constexpr uint8_t kO[7] = {14, 17, 17, 17, 17, 17, 14};
  static constexpr uint8_t kP[7] = {30, 17, 17, 30, 16, 16, 16};
  static constexpr uint8_t kQ[7] = {14, 17, 17, 17, 21, 18, 13};
  static constexpr uint8_t kR[7] = {30, 17, 17, 30, 20, 18, 17};
  static constexpr uint8_t kS[7] = {15, 16, 16, 14, 1, 1, 30};
  static constexpr uint8_t kT[7] = {31, 4, 4, 4, 4, 4, 4};
  static constexpr uint8_t kU[7] = {17, 17, 17, 17, 17, 17, 14};
  static constexpr uint8_t kV[7] = {17, 17, 17, 17, 17, 10, 4};
  static constexpr uint8_t kW[7] = {17, 17, 17, 21, 21, 21, 10};
  static constexpr uint8_t kX[7] = {17, 17, 10, 4, 10, 17, 17};
  static constexpr uint8_t kY[7] = {17, 17, 10, 4, 4, 4, 4};
  static constexpr uint8_t kZ[7] = {31, 1, 2, 4, 8, 16, 31};
  switch (character) {
  case '0':
    return k0;
  case '1':
    return k1;
  case '2':
    return k2;
  case '3':
    return k3;
  case '4':
    return k4;
  case '5':
    return k5;
  case '6':
    return k6;
  case '7':
    return k7;
  case '8':
    return k8;
  case '9':
    return k9;
  case 'A':
    return kA;
  case 'B':
    return kB;
  case 'C':
    return kC;
  case 'D':
    return kD;
  case 'E':
    return kE;
  case 'F':
    return kF;
  case 'G':
    return kG;
  case 'H':
    return kH;
  case 'I':
    return kI;
  case 'J':
    return kJ;
  case 'K':
    return kK;
  case 'L':
    return kL;
  case 'M':
    return kM;
  case 'N':
    return kN;
  case 'O':
    return kO;
  case 'P':
    return kP;
  case 'Q':
    return kQ;
  case 'R':
    return kR;
  case 'S':
    return kS;
  case 'T':
    return kT;
  case 'U':
    return kU;
  case 'V':
    return kV;
  case 'W':
    return kW;
  case 'X':
    return kX;
  case 'Y':
    return kY;
  case 'Z':
    return kZ;
  default:
    return kSpace;
  }
}

void drawText(uint32_t *pixels, int stride, int width, int y,
              const char *message, int scale, uint32_t color) {
  const int length = strlen(message);
  const int textWidth = length * 6 * scale - scale;
  const int startX = (width - textWidth) / 2;
  for (int index = 0; index < length; ++index) {
    const uint8_t *rows = glyph(message[index]);
    for (int row = 0; row < 7; ++row) {
      for (int column = 0; column < 5; ++column) {
        if ((rows[row] & (1 << (4 - column))) == 0)
          continue;
        for (int pixelY = 0; pixelY < scale; ++pixelY) {
          for (int pixelX = 0; pixelX < scale; ++pixelX) {
            pixels[(y + row * scale + pixelY) * stride + startX +
                   index * 6 * scale + column * scale + pixelX] = color;
          }
        }
      }
    }
  }
}

void fillRect(uint32_t *pixels, int stride, int width, int height, int left,
              int top, int right, int bottom, uint32_t color) {
  if (left < 0)
    left = 0;
  if (top < 0)
    top = 0;
  if (right > width)
    right = width;
  if (bottom > height)
    bottom = height;
  for (int y = top; y < bottom; ++y) {
    for (int x = left; x < right; ++x)
      pixels[y * stride + x] = color;
  }
}

void drawTextAt(uint32_t *pixels, int stride, int centerX, int y,
                const char *message, int scale, uint32_t color) {
  const int length = strlen(message);
  const int textWidth = length * 6 * scale - scale;
  const int startX = centerX - textWidth / 2;
  for (int index = 0; index < length; ++index) {
    const uint8_t *rows = glyph(message[index]);
    for (int row = 0; row < 7; ++row) {
      for (int column = 0; column < 5; ++column) {
        if ((rows[row] & (1 << (4 - column))) == 0)
          continue;
        for (int pixelY = 0; pixelY < scale; ++pixelY) {
          for (int pixelX = 0; pixelX < scale; ++pixelX) {
            pixels[(y + row * scale + pixelY) * stride + startX +
                   index * 6 * scale + column * scale + pixelX] = color;
          }
        }
      }
    }
  }
}

struct BridgeReply {
  int code = 0;
  int value = 0;
  bool unlocked = false;
};

bool writeAll(int fd, const void *data, size_t size) {
  const auto *cursor = static_cast<const uint8_t *>(data);
  while (size > 0) {
    const ssize_t written = write(fd, cursor, size);
    if (written <= 0)
      return false;
    cursor += written;
    size -= written;
  }
  return true;
}

bool readAll(int fd, void *data, size_t size) {
  auto *cursor = static_cast<uint8_t *>(data);
  while (size > 0) {
    const ssize_t received = read(fd, cursor, size);
    if (received <= 0)
      return false;
    cursor += received;
    size -= received;
  }
  return true;
}

BridgeReply bridgeRequest(int command, const std::string &pin = {}) {
  BridgeReply reply;
  const int fd = socket(AF_UNIX, SOCK_STREAM | SOCK_CLOEXEC, 0);
  if (fd < 0)
    return reply;
  timeval timeout{2, 0};
  setsockopt(fd, SOL_SOCKET, SO_RCVTIMEO, &timeout, sizeof(timeout));
  setsockopt(fd, SOL_SOCKET, SO_SNDTIMEO, &timeout, sizeof(timeout));
  sockaddr_un address{};
  address.sun_family = AF_UNIX;
  address.sun_path[0] = '\0';
  static_assert(sizeof(kBridgeSocket) < sizeof(address.sun_path));
  memcpy(address.sun_path + 1, kBridgeSocket, sizeof(kBridgeSocket) - 1);
  const socklen_t addressLength =
      offsetof(sockaddr_un, sun_path) + sizeof(kBridgeSocket);
  if (connect(fd, reinterpret_cast<sockaddr *>(&address), addressLength) != 0) {
    close(fd);
    return reply;
  }

  const uint32_t magic = htonl(kBridgeMagic);
  const uint8_t commandByte = static_cast<uint8_t>(command);
  if (!writeAll(fd, &magic, sizeof(magic)) ||
      !writeAll(fd, &commandByte, sizeof(commandByte))) {
    close(fd);
    return reply;
  }
  if (command == kBridgeVerifyPin) {
    const uint16_t length = htons(static_cast<uint16_t>(pin.size()));
    if (!writeAll(fd, &length, sizeof(length)) ||
        !writeAll(fd, pin.data(), pin.size())) {
      close(fd);
      return reply;
    }
  }

  uint32_t responseMagic = 0;
  uint8_t responseCode = 0;
  if (!readAll(fd, &responseMagic, sizeof(responseMagic)) ||
      ntohl(responseMagic) != kBridgeMagic ||
      !readAll(fd, &responseCode, sizeof(responseCode))) {
    close(fd);
    return reply;
  }
  reply.code = responseCode;
  uint32_t value = 0;
  if (!readAll(fd, &value, sizeof(value))) {
    reply.code = 0;
    close(fd);
    return reply;
  }
  reply.value = static_cast<int>(ntohl(value));
  if (command == kBridgeStatus) {
    uint8_t unlocked = 0;
    if (!readAll(fd, &unlocked, sizeof(unlocked)))
      reply.code = 0;
    reply.unlocked = unlocked != 0;
  }
  close(fd);
  return reply;
}

int runBridgeProbe() {
  const BridgeReply reply = bridgeRequest(kBridgeStatus);
  if (reply.code != kBridgeOk) {
    ALOGE("framework_bridge_probe result=unavailable");
    return 1;
  }
  ALOGI("framework_bridge_probe result=ready credential_type=%d "
        "user_unlocked=%s",
        reply.value, reply.unlocked ? "true" : "false");
  return 0;
}

int runCoreProviderAcceptanceProbe() {
  const std::string mode =
      android::base::GetProperty("debug.sos.core.provider_probe", "");
  if (mode.empty()) {
    ALOGE("core_provider_probe_invocation status=missing-mode");
    return 1;
  }
  void *library = dlopen(kExperienceLibrary, RTLD_NOW | RTLD_LOCAL);
  if (library == nullptr) {
    ALOGE("core_provider_probe_invocation status=library-unavailable");
    return 1;
  }
  dlerror();
  auto probe = reinterpret_cast<CoreProviderAcceptanceProbe>(
      dlsym(library, "sos_core_provider_acceptance_probe"));
  if (const char *error = dlerror(); error != nullptr || probe == nullptr) {
    ALOGE("core_provider_probe_invocation status=non-shipping-probe-absent");
    dlclose(library);
    return 2;
  }
  const int result = probe(mode.c_str());
  ALOGI("core_provider_probe_invocation status=complete exit_code=%d", result);
  dlclose(library);
  return result;
}

enum class CompatCommand { Lock, HomeFailed };

struct CompatRequest {
  CompatCommand command;
  int responseFd;
};

int createCompatControlSocket() {
  const int server = socket(AF_UNIX, SOCK_STREAM | SOCK_CLOEXEC, 0);
  if (server < 0) {
    ALOGE("native_compat_control_failed step=socket error=%s", strerror(errno));
    return -1;
  }
  sockaddr_un address{};
  address.sun_family = AF_UNIX;
  address.sun_path[0] = '\0';
  static_assert(sizeof(kControlSocket) < sizeof(address.sun_path));
  memcpy(address.sun_path + 1, kControlSocket, sizeof(kControlSocket) - 1);
  const socklen_t addressLength =
      offsetof(sockaddr_un, sun_path) + sizeof(kControlSocket);
  if (bind(server, reinterpret_cast<const sockaddr *>(&address),
           addressLength) != 0 ||
      listen(server, 4) != 0) {
    ALOGE("native_compat_control_failed step=bind-listen error=%s",
          strerror(errno));
    close(server);
    return -1;
  }
  ALOGI("native_compat_control_ready transport=local_socket peer=system");
  return server;
}

CompatRequest waitForCompatCommand(int server) {
  for (;;) {
    const int client = accept4(server, nullptr, nullptr, SOCK_CLOEXEC);
    if (client < 0) {
      if (errno == EINTR)
        continue;
      ALOGE("native_compat_control_accept_failed error=%s", strerror(errno));
      usleep(kServiceWaitMicros);
      continue;
    }
    ucred peer{};
    socklen_t peerLength = sizeof(peer);
    uint32_t magic = 0;
    uint8_t command = 0;
    const bool accepted =
        getsockopt(client, SOL_SOCKET, SO_PEERCRED, &peer, &peerLength) == 0 &&
        peer.uid == kSystemUid && readAll(client, &magic, sizeof(magic)) &&
        ntohl(magic) == kControlMagic &&
        readAll(client, &command, sizeof(command));
    if (!accepted) {
      close(client);
      ALOGW("native_compat_control_peer_rejected");
      continue;
    }
    if (command == kControlLock) {
      ALOGI("native_compat_control command=lock");
      return {CompatCommand::Lock, client};
    }
    if (command == kControlHomeFailed) {
      ALOGW("native_compat_control command=home-failed");
      return {CompatCommand::HomeFailed, client};
    }
    close(client);
    ALOGW("native_compat_control_unknown command=%u", command);
  }
}

void acknowledgeCompatRequest(int responseFd, bool ready) {
  if (responseFd < 0)
    return;
  const uint8_t response = ready ? 1 : 0;
  if (!writeAll(responseFd, &response, sizeof(response)))
    ALOGW("native_compat_control_ack_failed ready=%s",
          ready ? "true" : "false");
  close(responseFd);
}

bool requestCompatHome() {
  for (int attempt = 0; attempt < 100; ++attempt) {
    const BridgeReply reply = bridgeRequest(kBridgeStartHome);
    if (reply.code == kBridgeOk) {
      ALOGI("native_compat_home_request result=accepted");
      return true;
    }
    usleep(kServiceWaitMicros);
  }
  ALOGE("native_compat_home_request result=unavailable");
  return false;
}

int openNamedInput(const char *wantedName, input_absinfo *xRange = nullptr,
                   input_absinfo *yRange = nullptr) {
  for (int index = 0; index < 32; ++index) {
    const std::string path = "/dev/input/event" + std::to_string(index);
    const int fd = open(path.c_str(), O_RDONLY | O_NONBLOCK | O_CLOEXEC);
    if (fd < 0)
      continue;
    char name[80]{};
    if (ioctl(fd, EVIOCGNAME(sizeof(name)), name) < 0 ||
        strcmp(name, wantedName) != 0) {
      close(fd);
      continue;
    }
    if (ioctl(fd, EVIOCGRAB, 1) != 0) {
      ALOGE("trusted_input_grab_failed device=%s error=%s", wantedName,
            strerror(errno));
      close(fd);
      return -1;
    }
    if (xRange != nullptr &&
        ioctl(fd, EVIOCGABS(ABS_MT_POSITION_X), xRange) != 0) {
      close(fd);
      return -1;
    }
    if (yRange != nullptr &&
        ioctl(fd, EVIOCGABS(ABS_MT_POSITION_Y), yRange) != 0) {
      close(fd);
      return -1;
    }
    ALOGI("trusted_input_ready device=%s mode=exclusive", wantedName);
    return fd;
  }
  ALOGE("trusted_input_missing device=%s", wantedName);
  return -1;
}

bool updateRecoveryChord(const input_event &event, bool *volumeUp,
                         bool *volumeDown) {
  if (event.type != EV_KEY || (event.value != 0 && event.value != 1))
    return false;
  if (event.code == KEY_VOLUMEUP)
    *volumeUp = event.value == 1;
  if (event.code == KEY_VOLUMEDOWN)
    *volumeDown = event.value == 1;
  if (!*volumeUp || !*volumeDown)
    return false;
  ALOGW("native_recovery_chord action=reboot,recovery");
  if (!android::base::SetProperty("sys.powerctl", "reboot,recovery")) {
    ALOGE("native_recovery_chord_failed action=reboot,recovery");
    return false;
  }
  for (;;)
    pause();
}

void renderLocked(const NativeSurface &nativeSurface, size_t pinLength,
                  const char *status, bool coreOne,
                  int credentialType = kCredentialPin,
                  bool credentialStatusReady = true) {
  ANativeWindow_Buffer buffer{};
  ARect dirty{0, 0, nativeSurface.width, nativeSurface.height};
  ANativeWindow *window = nativeSurface.surface.get();
  ANativeWindow_setBuffersGeometry(window, nativeSurface.width,
                                   nativeSurface.height,
                                   PIXEL_FORMAT_RGBA_8888);
  if (ANativeWindow_lock(window, &buffer, &dirty) != 0)
    return;
  auto *pixels = static_cast<uint32_t *>(buffer.bits);
  fillRect(pixels, buffer.stride, nativeSurface.width, nativeSurface.height, 0,
           0, nativeSurface.width, nativeSurface.height, 0xff0c1411);
  fillRect(pixels, buffer.stride, nativeSurface.width, nativeSurface.height, 0,
           0, nativeSurface.width, nativeSurface.height / 6, 0xff17211d);
  const int scale = nativeSurface.width >= 1000 ? 8 : 5;
  drawText(pixels, buffer.stride, nativeSurface.width,
           nativeSurface.height / 18, coreOne ? "SOS CORE 1" : "SOS LOCK",
           scale, 0xff87cba5);
  drawText(pixels, buffer.stride, nativeSurface.width, nativeSurface.height / 5,
           status, scale, 0xfff0f3ed);
  if (coreOne) {
    drawText(pixels, buffer.stride, nativeSurface.width,
             nativeSurface.height / 3, "NO ZYGOTE", scale, 0xfff0f3ed);
    drawText(pixels, buffer.stride, nativeSurface.width,
             nativeSurface.height / 2, "CE DATA LOCKED", scale, 0xffdf9c62);
    drawText(pixels, buffer.stride, nativeSurface.width,
             (nativeSurface.height * 2) / 3, "VOLUME UP DOWN", scale,
             0xfff0f3ed);
    drawText(pixels, buffer.stride, nativeSurface.width,
             (nativeSurface.height * 5) / 6, "USE RECOVERY", scale, 0xfff0f3ed);
    ANativeWindow_unlockAndPost(window);
    return;
  }

  if (!credentialStatusReady) {
    drawText(pixels, buffer.stride, nativeSurface.width,
             nativeSurface.height / 3, "CHECKING CREDENTIAL", scale / 2,
             0xfff0f3ed);
    ANativeWindow_unlockAndPost(window);
    return;
  }
  if (credentialType == kCredentialNone) {
    const int left = nativeSurface.width / 12;
    const int right = (nativeSurface.width * 11) / 12;
    const int top = (nativeSurface.height * 3) / 5;
    const int bottom = (nativeSurface.height * 4) / 5;
    fillRect(pixels, buffer.stride, nativeSurface.width, nativeSurface.height,
             left, top, right, bottom, 0xff245b43);
    drawTextAt(pixels, buffer.stride, (left + right) / 2,
               (top + bottom) / 2 - (7 * scale) / 2, "ENTER", scale,
               0xfff0f3ed);
    ANativeWindow_unlockAndPost(window);
    return;
  }
  if (credentialType != kCredentialPin) {
    drawText(pixels, buffer.stride, nativeSurface.width,
             nativeSurface.height / 3, "USE RECOVERY", scale / 2, 0xffdf9c62);
    ANativeWindow_unlockAndPost(window);
    return;
  }

  std::string dots(pinLength, 'O');
  if (dots.empty())
    dots = "ENTER PIN";
  drawText(pixels, buffer.stride, nativeSurface.width, nativeSurface.height / 4,
           dots.c_str(), scale, 0xfff0f3ed);
  static constexpr const char *labels[12] = {
      "1", "2", "3", "4", "5", "6", "7", "8", "9", "CLEAR", "0", "ENTER"};
  const int left = nativeSurface.width / 12;
  const int right = (nativeSurface.width * 11) / 12;
  const int top = (nativeSurface.height * 3) / 10;
  const int bottom = (nativeSurface.height * 19) / 20;
  const int cellWidth = (right - left) / 3;
  const int cellHeight = (bottom - top) / 4;
  for (int index = 0; index < 12; ++index) {
    const int column = index % 3;
    const int row = index / 3;
    const int x0 = left + column * cellWidth + 5;
    const int y0 = top + row * cellHeight + 5;
    const int x1 = left + (column + 1) * cellWidth - 5;
    const int y1 = top + (row + 1) * cellHeight - 5;
    fillRect(pixels, buffer.stride, nativeSurface.width, nativeSurface.height,
             x0, y0, x1, y1, index >= 9 ? 0xff245b43 : 0xff17211d);
    const int labelScale = index >= 9 ? scale / 2 : scale;
    drawTextAt(pixels, buffer.stride, (x0 + x1) / 2,
               (y0 + y1) / 2 - (7 * labelScale) / 2, labels[index], labelScale,
               0xfff0f3ed);
  }
  ANativeWindow_unlockAndPost(window);
}

int lockedKeyAt(int x, int y, int width, int height, int credentialType,
                bool credentialStatusReady) {
  if (!credentialStatusReady)
    return -3;
  if (credentialType == kCredentialNone) {
    const int left = width / 12;
    const int right = (width * 11) / 12;
    const int top = (height * 3) / 5;
    const int bottom = (height * 4) / 5;
    return x >= left && x < right && y >= top && y < bottom ? -1 : -3;
  }
  if (credentialType != kCredentialPin)
    return -3;
  const int left = width / 12;
  const int right = (width * 11) / 12;
  const int top = (height * 3) / 10;
  const int bottom = (height * 19) / 20;
  if (x < left || x >= right || y < top || y >= bottom)
    return -3;
  int column = (x - left) * 3 / (right - left);
  int row = (y - top) * 4 / (bottom - top);
  const int index = row * 3 + column;
  if (index < 9)
    return index + 1;
  if (index == 9)
    return -2;
  if (index == 10)
    return 0;
  return -1;
}

int runPinUnlock(bool runtimeRelock = false, int responseFd = -1) {
  NativeSurface surface;
  if (!createSurface("SOS Trusted Lock", kLockLayer, &surface)) {
    acknowledgeCompatRequest(responseFd, false);
    return 1;
  }
  input_absinfo xRange{};
  input_absinfo yRange{};
  const int touch = openNamedInput("sec_touchscreen", &xRange, &yRange);
  if (touch < 0) {
    acknowledgeCompatRequest(responseFd, false);
    renderLocked(surface, 0, "INPUT FAILED", false);
    return 1;
  }
  const int keys = openNamedInput("gpio_keys");
  if (keys < 0) {
    close(touch);
    acknowledgeCompatRequest(responseFd, false);
    renderLocked(surface, 0, "KEYS FAILED", false);
    return 1;
  }
  acknowledgeCompatRequest(responseFd, true);
  ALOGI("trusted_lock_ready bridge=headless-framework credential=pending "
        "recovery_chord=volume_up+volume_down");
  std::string pin;
  std::string status = "WAITING";
  int credentialType = kCredentialNone;
  bool credentialStatusReady = false;
  bool volumeUp = false;
  bool volumeDown = false;
  int rawX = 0;
  int rawY = 0;
  for (;;) {
    if (!runtimeRelock &&
        android::base::GetBoolProperty("sys.user.0.ce_available", false)) {
      close(touch);
      close(keys);
      SurfaceComposerClient::Transaction{}.hide(surface.control).apply();
      ALOGI("trusted_unlock_complete ce_available=true");
      return 0;
    }
    if (!credentialStatusReady) {
      BridgeReply bridge = bridgeRequest(kBridgeStatus);
      if (bridge.code == kBridgeOk) {
        credentialType = bridge.value;
        credentialStatusReady = true;
        if (credentialType == kCredentialPin) {
          status = "PIN REQUIRED";
        } else if (credentialType == kCredentialNone) {
          status = runtimeRelock ? "PRESS ENTER" : "UNLOCKING";
        } else {
          status = "UNSUPPORTED";
        }
        ALOGI(
            "framework_bridge_status ready=true credential_type=%d unlocked=%s",
            credentialType, bridge.unlocked ? "true" : "false");
      }
    }
    renderLocked(surface, pin.size(), status.c_str(), false, credentialType,
                 credentialStatusReady);
    pollfd descriptors[2] = {{touch, POLLIN, 0}, {keys, POLLIN, 0}};
    if (poll(descriptors, 2, 100) <= 0)
      continue;
    if ((descriptors[1].revents & POLLIN) != 0) {
      input_event keyEvent{};
      while (read(keys, &keyEvent, sizeof(keyEvent)) == sizeof(keyEvent))
        updateRecoveryChord(keyEvent, &volumeUp, &volumeDown);
    }
    if ((descriptors[0].revents & POLLIN) == 0)
      continue;
    bool released = false;
    input_event event{};
    while (read(touch, &event, sizeof(event)) == sizeof(event)) {
      if (event.type == EV_ABS && event.code == ABS_MT_POSITION_X)
        rawX = event.value;
      if (event.type == EV_ABS && event.code == ABS_MT_POSITION_Y)
        rawY = event.value;
      if (event.type == EV_ABS && event.code == ABS_MT_TRACKING_ID &&
          event.value < 0)
        released = true;
      if (event.type == EV_KEY && event.code == BTN_TOUCH && event.value == 0)
        released = true;
      if (event.type != EV_SYN || event.code != SYN_REPORT || !released)
        continue;
      const int xDenominator = xRange.maximum - xRange.minimum;
      const int yDenominator = yRange.maximum - yRange.minimum;
      if (xDenominator <= 0 || yDenominator <= 0)
        continue;
      const int x = (rawX - xRange.minimum) * surface.width / xDenominator;
      const int y = (rawY - yRange.minimum) * surface.height / yDenominator;
      const int key = lockedKeyAt(x, y, surface.width, surface.height,
                                  credentialType, credentialStatusReady);
      if (key >= 0 && pin.size() < 64) {
        pin.push_back(static_cast<char>('0' + key));
        status = "ENTER PIN";
      } else if (key == -2) {
        std::fill(pin.begin(), pin.end(), '\0');
        pin.clear();
        status = "CLEARED";
      } else if (key == -1 && runtimeRelock && pin.empty() &&
                 credentialStatusReady && credentialType == kCredentialNone) {
        close(touch);
        close(keys);
        SurfaceComposerClient::Transaction{}.hide(surface.control).apply();
        ALOGI("native_runtime_unlock_complete credential=none");
        return 0;
      } else if (key == -1 && pin.size() >= 4 && credentialStatusReady &&
                 credentialType == kCredentialPin) {
        BridgeReply result = bridgeRequest(kBridgeVerifyPin, pin);
        std::fill(pin.begin(), pin.end(), '\0');
        pin.clear();
        if (result.code == kBridgeOk) {
          status = "UNLOCKING";
          ALOGI("trusted_credential_result matched=true");
          if (runtimeRelock) {
            close(touch);
            close(keys);
            SurfaceComposerClient::Transaction{}.hide(surface.control).apply();
            ALOGI("native_runtime_unlock_complete credential=pin");
            return 0;
          }
        } else if (result.code == kBridgeRetry) {
          status = "TRY LATER";
          ALOGW("trusted_credential_result throttled=true timeout_ms=%d",
                result.value);
        } else {
          status = "PIN REJECTED";
          ALOGW("trusted_credential_result matched=false");
        }
      }
      released = false;
    }
  }
}

int runCoreOneLocked() {
  NativeSurface surface;
  if (!createSurface("SOS Core 1 Locked", kLockLayer, &surface))
    return 1;
  renderLocked(surface, 0, "NATIVE RECOVERY", true);
  const int keys = openNamedInput("gpio_keys");
  ALOGW("core1_locked_surface_ready native_synthetic_password=false "
        "recovery_chord=volume_up+volume_down keys=%s",
        keys >= 0 ? "ready" : "missing");
  if (keys < 0) {
    for (;;)
      pause();
  }
  bool volumeUp = false;
  bool volumeDown = false;
  for (;;) {
    pollfd descriptor{keys, POLLIN, 0};
    if (poll(&descriptor, 1, -1) <= 0)
      continue;
    input_event event{};
    while (read(keys, &event, sizeof(event)) == sizeof(event))
      updateRecoveryChord(event, &volumeUp, &volumeDown);
  }
}

int runPreUnlockGate() {
  const std::string stage =
      android::base::GetProperty("ro.sos.core.stage", "shadow");
  if (stage == "compat" || stage == "0b")
    return runPinUnlock(false);
  if (stage == "1") {
    if (android::base::GetBoolProperty("debug.sos.core.lock", false))
      return runCoreOneLocked();
    // The stock experience and provider state live in device-encrypted
    // /data/misc/sos. Core 1 still has no native synthetic-password owner, so
    // starting here must not claim CE availability or unwrap user storage.
    // The locked surface remains available as an explicit diagnostic and the
    // fixed Recovery surface remains the supervisor's failure boundary.
    ALOGI("core1_experience_start ce_available=false "
          "native_synthetic_password=false de_storage=true");
    return 0;
  }
  ALOGW("native_core_deferred reason=user_ce_locked stage=%s", stage.c_str());
  return kExitAndroidUserLocked;
}

void renderRecovery(const NativeSurface &nativeSurface, bool androidAvailable) {
  ANativeWindow_Buffer buffer{};
  ARect dirty{0, 0, nativeSurface.width, nativeSurface.height};
  ANativeWindow *window = nativeSurface.surface.get();
  ANativeWindow_setBuffersGeometry(window, nativeSurface.width,
                                   nativeSurface.height,
                                   PIXEL_FORMAT_RGBA_8888);
  if (ANativeWindow_lock(window, &buffer, &dirty) != 0) {
    ALOGE("cannot lock the fixed recovery surface");
    return;
  }
  auto *pixels = static_cast<uint32_t *>(buffer.bits);
  for (int y = 0; y < nativeSurface.height; ++y) {
    for (int x = 0; x < nativeSurface.width; ++x) {
      pixels[y * buffer.stride + x] =
          recoveryColor(x, y, nativeSurface.width, nativeSurface.height);
    }
  }
  const int scale = nativeSurface.width >= 1000 ? 8 : 5;
  drawText(pixels, buffer.stride, nativeSurface.width,
           nativeSurface.height / 12, "SOS RECOVERY", scale, 0xff87cba5);
  drawText(pixels, buffer.stride, nativeSurface.width, nativeSurface.height / 4,
           "CORE STOPPED", scale, 0xfff0f3ed);
  drawText(pixels, buffer.stride, nativeSurface.width,
           (nativeSurface.height * 9) / 16, "VOLUME UP", scale, 0xfff0f3ed);
  drawText(pixels, buffer.stride, nativeSurface.width,
           (nativeSurface.height * 5) / 8, "RETRY SOS", scale, 0xfff0f3ed);
  drawText(pixels, buffer.stride, nativeSurface.width,
           (nativeSurface.height * 13) / 16, "VOLUME DOWN", scale, 0xfff0f3ed);
  drawText(pixels, buffer.stride, nativeSurface.width,
           (nativeSurface.height * 7) / 8,
           androidAvailable ? "SHOW ANDROID" : "USE RECOVERY", scale,
           0xfff0f3ed);
  ANativeWindow_unlockAndPost(window);
}

int openVolumeKeys() {
  for (int index = 0; index < 32; ++index) {
    const std::string path = "/dev/input/event" + std::to_string(index);
    const int fd = open(path.c_str(), O_RDONLY | O_NONBLOCK | O_CLOEXEC);
    if (fd < 0)
      continue;
    char name[80]{};
    if (ioctl(fd, EVIOCGNAME(sizeof(name)), name) >= 0 &&
        strcmp(name, "gpio_keys") == 0) {
      if (ioctl(fd, EVIOCGRAB, 1) != 0) {
        ALOGE("fixed_recovery_key_grab_failed error=%s", strerror(errno));
        close(fd);
        return -1;
      }
      return fd;
    }
    close(fd);
  }
  return -1;
}

enum class RecoveryAction { Retry, Android };

RecoveryAction runRecovery(int childStatus, bool androidAvailable) {
  NativeSurface nativeSurface;
  if (!createSurface("SOS Fixed Recovery", kRecoveryLayer, &nativeSurface)) {
    ALOGE("fixed recovery surface unavailable; returning to Android");
    return androidAvailable ? RecoveryAction::Android : RecoveryAction::Retry;
  }
  renderRecovery(nativeSurface, androidAvailable);
  const int keys = openVolumeKeys();
  ALOGW("fixed_recovery_ready child_status=%d volume_up=retry "
        "volume_down=android keys=%s",
        childStatus, keys >= 0 ? "ready" : "missing");
  if (keys < 0) {
    if (androidAvailable) {
      ALOGE("fixed_recovery_keys_unavailable action=android");
      usleep(kServiceWaitMicros);
      return RecoveryAction::Android;
    }
    ALOGE("fixed_recovery_keys_unavailable action=hold_headless_recovery");
    std::string lastCommand =
        android::base::GetProperty("debug.sos.core.recovery", "");
    for (;;) {
      const std::string command =
          android::base::GetProperty("debug.sos.core.recovery", "");
      if (command != lastCommand && command == "retry")
        return RecoveryAction::Retry;
      lastCommand = command;
      usleep(kServiceWaitMicros);
    }
  }
  bool up = false;
  bool down = false;
  std::string lastCommand =
      android::base::GetProperty("debug.sos.core.recovery", "");
  for (;;) {
    const std::string command =
        android::base::GetProperty("debug.sos.core.recovery", "");
    if (command != lastCommand) {
      lastCommand = command;
      if (command == "retry") {
        if (keys >= 0)
          close(keys);
        return RecoveryAction::Retry;
      }
      if (command == "android") {
        if (!androidAvailable) {
          ALOGW("fixed_recovery_android_unavailable stage=headless");
          continue;
        }
        if (keys >= 0)
          close(keys);
        return RecoveryAction::Android;
      }
    }
    pollfd descriptor{keys, POLLIN, 0};
    if (keys < 0 || poll(&descriptor, 1, 100) <= 0)
      continue;
    input_event event{};
    while (read(keys, &event, sizeof(event)) == sizeof(event)) {
      if (event.type != EV_KEY || (event.value != 0 && event.value != 1)) {
        continue;
      }
      if (event.code == KEY_VOLUMEUP)
        up = event.value == 1;
      if (event.code == KEY_VOLUMEDOWN)
        down = event.value == 1;
      if (down) {
        if (!androidAvailable) {
          ALOGW("fixed_recovery_android_unavailable stage=headless");
          down = false;
          continue;
        }
        close(keys);
        return RecoveryAction::Android;
      }
      if (up) {
        close(keys);
        return RecoveryAction::Retry;
      }
    }
  }
}

int superviseCompatUnlocked() {
  int server = createCompatControlSocket();
  while (server < 0) {
    runRecovery(70, false);
    server = createCompatControlSocket();
  }
  while (!requestCompatHome())
    runRecovery(70, false);

  for (;;) {
    const CompatRequest request = waitForCompatCommand(server);
    if (request.command == CompatCommand::Lock) {
      int lockStatus = runPinUnlock(true, request.responseFd);
      while (lockStatus != 0) {
        runRecovery(70, false);
        lockStatus = runPinUnlock(true);
      }
      continue;
    }

    acknowledgeCompatRequest(request.responseFd, true);
    ALOGW("native_compat_home_failed action=restart-home");
    if (requestCompatHome())
      continue;

    ALOGE("native_compat_home_restart_failed action=fixed-recovery");
    do {
      runRecovery(70, false);
    } while (!requestCompatHome());
  }
}

int supervise() {
  const std::string stage =
      android::base::GetProperty("ro.sos.core.stage", "shadow");
  const bool androidFallbackAvailable = stage == "shadow" || stage == "0a";
  bool faultWasRequested =
      android::base::GetBoolProperty("debug.sos.core.fault", false);
  for (;;) {
    const pid_t child = fork();
    if (child < 0) {
      ALOGE("cannot fork the GPUI host: %s", strerror(errno));
      return 1;
    }
    if (child == 0) {
      execl(kHostExecutable, "sos-core-host", "--experience-child", nullptr);
      ALOGE("cannot exec the GPUI child: %s", strerror(errno));
      _exit(127);
    }

    ALOGI("native_supervisor_ready child=%d", child);
    int status = 0;
    for (;;) {
      const pid_t waited = waitpid(child, &status, WNOHANG);
      if (waited == child)
        break;
      if (waited < 0) {
        ALOGE("waitpid failed: %s", strerror(errno));
        return 1;
      }
      const bool faultRequested =
          android::base::GetBoolProperty("debug.sos.core.fault", false);
      if (faultRequested && !faultWasRequested) {
        ALOGW("native_fault_injected child=%d signal=%d", child, SIGABRT);
        kill(child, SIGABRT);
      }
      faultWasRequested = faultRequested;
      usleep(kServiceWaitMicros);
    }

    if (WIFEXITED(status) && WEXITSTATUS(status) == 0)
      return 0;
    if (WIFEXITED(status) && WEXITSTATUS(status) == kExitCompatHandoff) {
      ALOGI("native_compat_supervisor state=unlocked-wait");
      return superviseCompatUnlocked();
    }
    if (WIFEXITED(status) && WEXITSTATUS(status) == kExitAndroidUserLocked) {
      ALOGW("native_core_fallback reason=user_ce_locked");
      return 0;
    }
    if (WIFEXITED(status) && WEXITSTATUS(status) == kExitRecoveryRequested) {
      ALOGW("native recovery chord requested the fixed recovery surface");
    } else if (WIFSIGNALED(status)) {
      ALOGE("native_gpui_failed signal=%d", WTERMSIG(status));
    } else {
      ALOGE("native_gpui_failed status=%d", status);
    }
    if (runRecovery(status, androidFallbackAvailable) ==
        RecoveryAction::Android) {
      ALOGW("fixed_recovery_action action=android");
      return 0;
    }
    ALOGW("fixed_recovery_action action=retry");
  }
}

} // namespace

int main(int argc, char **argv) {
  if (argc == 2 && strcmp(argv[1], "--experience-child") == 0) {
    return runExperience();
  }
  if (argc == 2 && strcmp(argv[1], "--bridge-probe") == 0) {
    if (android::base::GetProperty("ro.sos.providers", "") == "core-native") {
      return runCoreProviderAcceptanceProbe();
    }
    return runBridgeProbe();
  }
  return supervise();
}
