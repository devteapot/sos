#include <android/log.h>
#include <cutils/sockets.h>
#include <fcntl.h>
#include <linux/input.h>
#include <linux/uinput.h>
#include <sys/socket.h>
#include <sys/types.h>
#include <unistd.h>

#include <algorithm>
#include <cerrno>
#include <chrono>
#include <cstdio>
#include <cstdlib>
#include <cstring>
#include <string>
#include <thread>

namespace {

constexpr char kLogTag[] = "sos-core-input";
constexpr char kSocketName[] = "sos_core_input_automation";
constexpr char kDeviceName[] = "sos_core_automation_touch";
constexpr uid_t kShellUid = 2000;
constexpr int kWidth = 1080;
constexpr int kHeight = 2400;
constexpr size_t kMaxRequestBytes = 128;
constexpr size_t kMaxResponseBytes = 1024;

#define LOGI(...) __android_log_print(ANDROID_LOG_INFO, kLogTag, __VA_ARGS__)
#define LOGW(...) __android_log_print(ANDROID_LOG_WARN, kLogTag, __VA_ARGS__)
#define LOGE(...) __android_log_print(ANDROID_LOG_ERROR, kLogTag, __VA_ARGS__)

bool WriteAll(int fd, const void* data, size_t size) {
    const auto* cursor = static_cast<const char*>(data);
    while (size > 0) {
        const ssize_t written = TEMP_FAILURE_RETRY(write(fd, cursor, size));
        if (written <= 0) {
            return false;
        }
        cursor += written;
        size -= static_cast<size_t>(written);
    }
    return true;
}

bool WriteEvent(int fd, uint16_t type, uint16_t code, int32_t value) {
    input_event event{};
    event.type = type;
    event.code = code;
    event.value = value;
    return WriteAll(fd, &event, sizeof(event));
}

bool ConfigureAbsoluteAxis(int fd, uint16_t code, int minimum, int maximum) {
    uinput_abs_setup setup{};
    setup.code = code;
    setup.absinfo.minimum = minimum;
    setup.absinfo.maximum = maximum;
    return ioctl(fd, UI_ABS_SETUP, &setup) == 0;
}

int CreateTouchDevice() {
    const int fd = TEMP_FAILURE_RETRY(open("/dev/uinput", O_WRONLY | O_NONBLOCK | O_CLOEXEC));
    if (fd < 0) {
        LOGE("core_input_automation_failed stage=open-uinput errno=%d", errno);
        return -1;
    }
    const bool event_bits = ioctl(fd, UI_SET_EVBIT, EV_SYN) == 0 &&
                            ioctl(fd, UI_SET_EVBIT, EV_KEY) == 0 &&
                            ioctl(fd, UI_SET_KEYBIT, BTN_TOUCH) == 0 &&
                            ioctl(fd, UI_SET_EVBIT, EV_ABS) == 0 &&
                            ioctl(fd, UI_SET_ABSBIT, ABS_MT_SLOT) == 0 &&
                            ioctl(fd, UI_SET_ABSBIT, ABS_MT_TRACKING_ID) == 0 &&
                            ioctl(fd, UI_SET_ABSBIT, ABS_MT_POSITION_X) == 0 &&
                            ioctl(fd, UI_SET_ABSBIT, ABS_MT_POSITION_Y) == 0 &&
                            ioctl(fd, UI_SET_PROPBIT, INPUT_PROP_DIRECT) == 0;
    const bool axes = ConfigureAbsoluteAxis(fd, ABS_MT_SLOT, 0, 9) &&
                      ConfigureAbsoluteAxis(fd, ABS_MT_TRACKING_ID, 0, 65535) &&
                      ConfigureAbsoluteAxis(fd, ABS_MT_POSITION_X, 0, kWidth - 1) &&
                      ConfigureAbsoluteAxis(fd, ABS_MT_POSITION_Y, 0, kHeight - 1);
    uinput_setup setup{};
    std::snprintf(setup.name, sizeof(setup.name), "%s", kDeviceName);
    setup.id.bustype = BUS_VIRTUAL;
    setup.id.vendor = 0x534f;
    setup.id.product = 0x5304;
    setup.id.version = 1;
    if (!event_bits || !axes || ioctl(fd, UI_DEV_SETUP, &setup) != 0 ||
        ioctl(fd, UI_DEV_CREATE) != 0) {
        LOGE("core_input_automation_failed stage=create-device errno=%d", errno);
        close(fd);
        return -1;
    }
    std::this_thread::sleep_for(std::chrono::milliseconds(250));
    return fd;
}

bool EmitTap(int fd, int x, int y, uint64_t sequence) {
    const int tracking_id = static_cast<int>((sequence % 65535) + 1);
    const bool down = WriteEvent(fd, EV_ABS, ABS_MT_SLOT, 0) &&
                      WriteEvent(fd, EV_ABS, ABS_MT_TRACKING_ID, tracking_id) &&
                      WriteEvent(fd, EV_ABS, ABS_MT_POSITION_X, x) &&
                      WriteEvent(fd, EV_ABS, ABS_MT_POSITION_Y, y) &&
                      WriteEvent(fd, EV_KEY, BTN_TOUCH, 1) &&
                      WriteEvent(fd, EV_SYN, SYN_REPORT, 0);
    std::this_thread::sleep_for(std::chrono::milliseconds(24));
    const bool up = WriteEvent(fd, EV_ABS, ABS_MT_SLOT, 0) &&
                    WriteEvent(fd, EV_ABS, ABS_MT_TRACKING_ID, -1) &&
                    WriteEvent(fd, EV_KEY, BTN_TOUCH, 0) &&
                    WriteEvent(fd, EV_SYN, SYN_REPORT, 0);
    return down && up;
}

bool ReadRequest(int fd, std::string* request) {
    request->clear();
    char byte = '\0';
    while (request->size() <= kMaxRequestBytes) {
        const ssize_t result = TEMP_FAILURE_RETRY(read(fd, &byte, 1));
        if (result == 0) {
            return !request->empty();
        }
        if (result < 0) {
            return false;
        }
        if (byte == '\n') {
            return true;
        }
        request->push_back(byte);
    }
    return false;
}

void HandleClient(int client, int uinput_fd, uint64_t* sequence) {
    ucred credentials{};
    socklen_t credentials_size = sizeof(credentials);
    if (getsockopt(client, SOL_SOCKET, SO_PEERCRED, &credentials, &credentials_size) != 0 ||
        (credentials.uid != kShellUid && credentials.uid != 0)) {
        LOGW("core_input_automation_rejected reason=peer-uid uid=%u",
             static_cast<unsigned>(credentials.uid));
        const std::string response = "{\"ok\":false,\"error\":\"peer-not-authorized\"}\n";
        WriteAll(client, response.data(), response.size());
        return;
    }

    timeval timeout{};
    timeout.tv_sec = 2;
    setsockopt(client, SOL_SOCKET, SO_RCVTIMEO, &timeout, sizeof(timeout));
    std::string request;
    if (!ReadRequest(client, &request)) {
        const std::string response = "{\"ok\":false,\"error\":\"invalid-request\"}\n";
        WriteAll(client, response.data(), response.size());
        return;
    }
    if (request == "status") {
        const std::string response =
                "{\"ok\":true,\"version\":1,\"device\":\"sos_core_automation_touch\","
                "\"origin\":\"uinput\"}\n";
        WriteAll(client, response.data(), response.size());
        return;
    }

    int x = -1;
    int y = -1;
    char trailing = '\0';
    if (std::sscanf(request.c_str(), "tap %d %d %c", &x, &y, &trailing) != 2 || x < 0 ||
        x >= kWidth || y < 0 || y >= kHeight) {
        const std::string response = "{\"ok\":false,\"error\":\"invalid-tap\"}\n";
        WriteAll(client, response.data(), response.size());
        return;
    }

    ++*sequence;
    if (!EmitTap(uinput_fd, x, y, *sequence)) {
        LOGE("core_input_automation_failed stage=emit sequence=%llu errno=%d",
             static_cast<unsigned long long>(*sequence), errno);
        const std::string response = "{\"ok\":false,\"error\":\"emit-failed\"}\n";
        WriteAll(client, response.data(), response.size());
        return;
    }
    LOGI("core_input_automation_tap sequence=%llu x=%d y=%d peer_uid=%u origin=uinput",
         static_cast<unsigned long long>(*sequence), x, y,
         static_cast<unsigned>(credentials.uid));
    char response[160];
    const int response_size = std::snprintf(
            response, sizeof(response),
            "{\"ok\":true,\"sequence\":%llu,\"action\":\"tap\",\"x\":%d,\"y\":%d}\n",
            static_cast<unsigned long long>(*sequence), x, y);
    if (response_size > 0 && static_cast<size_t>(response_size) < sizeof(response)) {
        WriteAll(client, response, static_cast<size_t>(response_size));
    }
}

int RunDaemon() {
    const int control_fd = android_get_control_socket(kSocketName);
    if (control_fd < 0) {
        LOGE("core_input_automation_failed stage=control-socket errno=%d", errno);
        return 1;
    }
    const int uinput_fd = CreateTouchDevice();
    if (uinput_fd < 0) {
        return 1;
    }
    if (listen(control_fd, 4) != 0) {
        LOGE("core_input_automation_failed stage=listen errno=%d", errno);
        close(uinput_fd);
        return 1;
    }
    LOGI("core_input_automation_ready version=1 device=%s bounds=%dx%d transport=init-socket",
         kDeviceName, kWidth, kHeight);
    uint64_t sequence = 0;
    while (true) {
        const int client = TEMP_FAILURE_RETRY(accept4(control_fd, nullptr, nullptr, SOCK_CLOEXEC));
        if (client < 0) {
            LOGE("core_input_automation_failed stage=accept errno=%d", errno);
            close(uinput_fd);
            return 1;
        }
        HandleClient(client, uinput_fd, &sequence);
        close(client);
    }
}

int RunClient(int argc, char** argv) {
    if (argc != 2 && argc != 4) {
        std::fprintf(stderr, "usage: sos-core-inputctl status | tap X Y\n");
        return 2;
    }
    std::string request;
    if (argc == 2 && std::strcmp(argv[1], "status") == 0) {
        request = "status\n";
    } else if (argc == 4 && std::strcmp(argv[1], "tap") == 0) {
        char* x_end = nullptr;
        char* y_end = nullptr;
        const long x = std::strtol(argv[2], &x_end, 10);
        const long y = std::strtol(argv[3], &y_end, 10);
        if (x_end == argv[2] || *x_end != '\0' || y_end == argv[3] || *y_end != '\0' || x < 0 ||
            x >= kWidth || y < 0 || y >= kHeight) {
            std::fprintf(stderr, "tap coordinates must be within 0..1079 and 0..2399\n");
            return 2;
        }
        request = "tap " + std::to_string(x) + " " + std::to_string(y) + "\n";
    } else {
        std::fprintf(stderr, "usage: sos-core-inputctl status | tap X Y\n");
        return 2;
    }

    const int fd = socket_local_client(kSocketName, ANDROID_SOCKET_NAMESPACE_RESERVED, SOCK_STREAM);
    if (fd < 0) {
        std::fprintf(stderr, "SOS Core input automation is unavailable: %s\n", std::strerror(errno));
        return 1;
    }
    if (!WriteAll(fd, request.data(), request.size()) || shutdown(fd, SHUT_WR) != 0) {
        std::fprintf(stderr, "SOS Core input automation request failed: %s\n", std::strerror(errno));
        close(fd);
        return 1;
    }
    std::string response;
    char buffer[256];
    while (response.size() <= kMaxResponseBytes) {
        const ssize_t bytes = TEMP_FAILURE_RETRY(read(fd, buffer, sizeof(buffer)));
        if (bytes == 0) {
            break;
        }
        if (bytes < 0) {
            std::fprintf(stderr, "SOS Core input automation response failed: %s\n",
                         std::strerror(errno));
            close(fd);
            return 1;
        }
        response.append(buffer, static_cast<size_t>(bytes));
    }
    close(fd);
    if (response.size() > kMaxResponseBytes || response.find("\"ok\":true") == std::string::npos) {
        std::fwrite(response.data(), 1, response.size(), stderr);
        return 1;
    }
    std::fwrite(response.data(), 1, response.size(), stdout);
    return 0;
}

}  // namespace

int main(int argc, char** argv) {
    if (argc == 2 && std::strcmp(argv[1], "--daemon") == 0) {
        return RunDaemon();
    }
    return RunClient(argc, argv);
}
