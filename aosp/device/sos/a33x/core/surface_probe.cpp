#define LOG_TAG "SosCoreSurfaceProbe"

#include <EGL/egl.h>
#include <GLES2/gl2.h>
#include <binder/IServiceManager.h>
#include <binder/ProcessState.h>
#include <gui/ISurfaceComposer.h>
#include <gui/Surface.h>
#include <gui/SurfaceComposerClient.h>
#include <log/log.h>
#include <pthread.h>
#include <signal.h>
#include <ui/DisplayMode.h>
#include <ui/GraphicTypes.h>
#include <ui/PixelFormat.h>
#include <unistd.h>
#include <utils/Errors.h>
#include <utils/String8.h>
#include <utils/StrongPointer.h>

#include <cstdint>

namespace {

using android::IBinder;
using android::IServiceManager;
using android::ISurfaceComposerClient;
using android::PIXEL_FORMAT_RGB_565;
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
constexpr int32_t kProbeLayer = 0x40000001;

void waitForSurfaceFlinger() {
  const sp<IServiceManager> services = android::defaultServiceManager();
  const String16 name("SurfaceFlinger");
  while (services->checkService(name) == nullptr) {
    usleep(kServiceWaitMicros);
  }
}

EGLConfig chooseConfig(EGLDisplay display) {
  constexpr EGLint attributes[] = {
      EGL_RENDERABLE_TYPE,
      EGL_OPENGL_ES2_BIT,
      EGL_RED_SIZE,
      8,
      EGL_GREEN_SIZE,
      8,
      EGL_BLUE_SIZE,
      8,
      EGL_DEPTH_SIZE,
      0,
      EGL_NONE,
  };
  EGLConfig config = nullptr;
  EGLint count = 0;
  if (eglChooseConfig(display, attributes, &config, 1, &count) != EGL_TRUE ||
      count != 1) {
    return nullptr;
  }
  return config;
}

int run() {
  sigset_t signals;
  sigemptyset(&signals);
  sigaddset(&signals, SIGINT);
  sigaddset(&signals, SIGTERM);
  pthread_sigmask(SIG_BLOCK, &signals, nullptr);

  const sp<ProcessState> process = ProcessState::self();
  process->startThreadPool();
  waitForSurfaceFlinger();

  const auto displayIds = SurfaceComposerClient::getPhysicalDisplayIds();
  if (displayIds.empty()) {
    ALOGE("no physical display is available");
    return 1;
  }
  const sp<IBinder> displayToken =
      SurfaceComposerClient::getPhysicalDisplayToken(displayIds.front());
  if (displayToken == nullptr) {
    ALOGE("the primary display has no SurfaceFlinger token");
    return 1;
  }

  DisplayMode mode;
  const status_t modeStatus =
      SurfaceComposerClient::getActiveDisplayMode(displayToken, &mode);
  if (modeStatus != android::NO_ERROR) {
    ALOGE("cannot read the active display mode: %d", modeStatus);
    return 1;
  }

  const sp<SurfaceComposerClient> client = sp<SurfaceComposerClient>::make();
  if (client->initCheck() != android::NO_ERROR) {
    ALOGE("cannot connect to SurfaceFlinger");
    return 1;
  }
  const int32_t width = mode.resolution.getWidth();
  const int32_t height = mode.resolution.getHeight();
  const sp<SurfaceControl> control = client->createSurface(
      String8("SOS Core Surface Probe"), width, height, PIXEL_FORMAT_RGB_565,
      ISurfaceComposerClient::eOpaque);
  if (control == nullptr || !control->isValid()) {
    ALOGE("cannot create the SOS Core surface");
    return 1;
  }

  const sp<Surface> surface = control->getSurface();
  if (surface == nullptr) {
    ALOGE("SurfaceFlinger did not expose an ANativeWindow");
    return 1;
  }

  const EGLDisplay eglDisplay = eglGetDisplay(EGL_DEFAULT_DISPLAY);
  if (eglDisplay == EGL_NO_DISPLAY ||
      eglInitialize(eglDisplay, nullptr, nullptr) != EGL_TRUE) {
    ALOGE("cannot initialize EGL: 0x%x", eglGetError());
    return 1;
  }
  const EGLConfig config = chooseConfig(eglDisplay);
  if (config == nullptr) {
    ALOGE("cannot choose an EGL config: 0x%x", eglGetError());
    eglTerminate(eglDisplay);
    return 1;
  }
  constexpr EGLint contextAttributes[] = {EGL_CONTEXT_CLIENT_VERSION, 2,
                                          EGL_NONE};
  const EGLContext context =
      eglCreateContext(eglDisplay, config, EGL_NO_CONTEXT, contextAttributes);
  const EGLSurface eglSurface =
      eglCreateWindowSurface(eglDisplay, config, surface.get(), nullptr);
  if (context == EGL_NO_CONTEXT || eglSurface == EGL_NO_SURFACE ||
      eglMakeCurrent(eglDisplay, eglSurface, eglSurface, context) != EGL_TRUE) {
    ALOGE("cannot bind the native SurfaceComposer window to EGL: 0x%x",
          eglGetError());
    if (eglSurface != EGL_NO_SURFACE) {
      eglDestroySurface(eglDisplay, eglSurface);
    }
    if (context != EGL_NO_CONTEXT) {
      eglDestroyContext(eglDisplay, context);
    }
    eglTerminate(eglDisplay);
    return 1;
  }

  SurfaceComposerClient::Transaction{}
      .setLayer(control, kProbeLayer)
      .show(control)
      .apply();
  glViewport(0, 0, width, height);
  glClearColor(0.025f, 0.035f, 0.055f, 1.0f);
  glClear(GL_COLOR_BUFFER_BIT);
  if (eglSwapBuffers(eglDisplay, eglSurface) != EGL_TRUE) {
    ALOGE("cannot present the probe frame: 0x%x", eglGetError());
    return 1;
  }
  ALOGI("native_surface_ready width=%d height=%d ui_owner=android-shadow",
        width, height);

  int received = 0;
  sigwait(&signals, &received);
  ALOGI("stopping after signal=%d", received);

  SurfaceComposerClient::Transaction{}.hide(control).apply();
  eglMakeCurrent(eglDisplay, EGL_NO_SURFACE, EGL_NO_SURFACE, EGL_NO_CONTEXT);
  eglDestroySurface(eglDisplay, eglSurface);
  eglDestroyContext(eglDisplay, context);
  eglTerminate(eglDisplay);
  return 0;
}

} // namespace

int main() { return run(); }
