#include <aidl/android/hardware/health/BatteryStatus.h>
#include <aidl/android/hardware/health/HealthInfo.h>
#include <aidl/android/hardware/health/IHealth.h>
#include <aidl/android/hardware/wifi/supplicant/ISupplicant.h>
#include <aidl/android/hardware/wifi/supplicant/ISupplicantStaIface.h>
#include <aidl/android/hardware/wifi/supplicant/ISupplicantStaNetwork.h>
#include <aidl/android/hardware/wifi/supplicant/SignalPollResult.h>
#include <android-base/file.h>
#include <android-base/logging.h>
#include <android-base/strings.h>
#include <android/binder_manager.h>
#include <arpa/inet.h>
#include <dirent.h>
#include <fcntl.h>
#include <json/json.h>
#include <media/AudioSystem.h>
#include <openssl/sha.h>
#include <sys/socket.h>
#include <sys/stat.h>
#include <sys/types.h>
#include <sys/un.h>
#include <system/audio.h>
#include <unistd.h>

#include <algorithm>
#include <array>
#include <cctype>
#include <cerrno>
#include <chrono>
#include <climits>
#include <cstdint>
#include <cstring>
#include <fstream>
#include <memory>
#include <optional>
#include <set>
#include <sstream>
#include <string>
#include <string_view>
#include <utility>
#include <vector>

#include "socket_io.h"

namespace {

using aidl::android::hardware::health::BatteryStatus;
using aidl::android::hardware::health::HealthInfo;
using aidl::android::hardware::health::IHealth;
using aidl::android::hardware::wifi::supplicant::ISupplicant;
using aidl::android::hardware::wifi::supplicant::ISupplicantStaIface;
using aidl::android::hardware::wifi::supplicant::ISupplicantStaNetwork;
using aidl::android::hardware::wifi::supplicant::SignalPollResult;

constexpr uint32_t kMagic = 0x534f5331;
constexpr uint32_t kProviderAbi = 1;
constexpr uint8_t kResponseOk = 1;
constexpr uint8_t kResponseError = 0;
constexpr size_t kMaxRequestBytes = 1024 * 1024;
constexpr size_t kMaxLabelBytes = 256;
constexpr size_t kMaxItems = 64;
constexpr int kMusicVolumeMaximum = 25;
constexpr char kSocketName[] = "sos_core_platform";
constexpr char kStateDirectory[] = "/data/misc/sos/platform";
constexpr char kAudioStatePath[] = "/data/misc/sos/platform/audio.json";
constexpr char kMediaStatePath[] = "/data/misc/sos/platform/media.json";
constexpr char kAttentionStatePath[] = "/data/misc/sos/platform/attention.json";
constexpr char kNetworkStatePath[] = "/data/misc/sos/platform/network.json";
constexpr char kAppsManifestPath[] = "/system_ext/etc/sos/core-apps.json";

struct ScopedFd {
    explicit ScopedFd(int value = -1) : value(value) {}
    ~ScopedFd() {
        if (value >= 0) close(value);
    }
    ScopedFd(const ScopedFd&) = delete;
    ScopedFd& operator=(const ScopedFd&) = delete;
    ScopedFd(ScopedFd&& other) noexcept : value(std::exchange(other.value, -1)) {}
    ScopedFd& operator=(ScopedFd&& other) noexcept {
        if (this != &other) {
            if (value >= 0) close(value);
            value = std::exchange(other.value, -1);
        }
        return *this;
    }
    int value;
};

struct NativeApp {
    std::string id;
    std::string label;
    std::string target;
};

struct SavedNetwork {
    int supplicant_id = -1;
    std::string id;
    std::string label;
    bool connected = false;
};

uint64_t nowMs() {
    const auto now = std::chrono::system_clock::now().time_since_epoch();
    return static_cast<uint64_t>(
            std::chrono::duration_cast<std::chrono::milliseconds>(now).count());
}

std::string bounded(std::string value) {
    for (char& character : value) {
        if (character == '\n' || character == '\r' || character == '\t') character = ' ';
    }
    value = android::base::Trim(value);
    if (value.size() > kMaxLabelBytes) value.resize(kMaxLabelBytes);
    return value;
}

std::string opaque(std::string_view kind, std::string_view identity) {
    std::array<unsigned char, SHA256_DIGEST_LENGTH> digest{};
    SHA256(reinterpret_cast<const unsigned char*>(identity.data()), identity.size(), digest.data());
    static constexpr char kHex[] = "0123456789abcdef";
    std::string result(kind);
    result.push_back('-');
    for (size_t index = 0; index < 12; ++index) {
        result.push_back(kHex[digest[index] >> 4]);
        result.push_back(kHex[digest[index] & 0x0f]);
    }
    return result;
}

bool validInternalTarget(std::string_view value) {
    return !value.empty() && value.size() <= 96 &&
            std::all_of(value.begin(), value.end(), [](unsigned char character) {
                return std::isalnum(character) || character == '.' || character == '_' ||
                        character == '-';
            });
}

Json::Value nullValue() {
    return Json::Value(Json::nullValue);
}

std::optional<Json::Value> readJson(const char* path) {
    std::ifstream input(path);
    if (!input) return std::nullopt;
    Json::CharReaderBuilder builder;
    Json::Value value;
    std::string error;
    if (!Json::parseFromStream(builder, input, &value, &error)) {
        LOG(WARNING) << "core_platform_json_rejected path=" << path;
        return std::nullopt;
    }
    return value;
}

std::string encodeJson(const Json::Value& value) {
    Json::StreamWriterBuilder builder;
    builder["indentation"] = "";
    return Json::writeString(builder, value);
}

bool writeJsonAtomically(const char* path, const Json::Value& value) {
    const std::string temporary = std::string(path) + ".new";
    if (!android::base::WriteStringToFile(encodeJson(value), temporary, 0600, 1000, 1000, true)) {
        return false;
    }
    if (rename(temporary.c_str(), path) != 0) {
        unlink(temporary.c_str());
        return false;
    }
    return true;
}

std::optional<std::string> readTrimmed(const std::string& path) {
    std::string value;
    if (!android::base::ReadFileToString(path, &value)) return std::nullopt;
    value = android::base::Trim(value);
    if (value.empty()) return std::nullopt;
    return value;
}

Json::Value emptyClock() {
    Json::Value clock(Json::objectValue);
    clock["unix_time_ms"] = Json::UInt64(0);
    clock["locale"] = "";
    clock["timezone"] = "";
    clock["time_label"] = "";
    clock["date_label"] = "";
    return clock;
}

Json::Value healthSnapshot() {
    Json::Value power(Json::objectValue);
    power["battery_percent"] = nullValue();
    power["charging"] = nullValue();
    power["charging_source"] = "";
    power["battery_temperature_deci_c"] = nullValue();
    power["thermal_status"] = nullValue();

    const std::string serviceName = std::string(IHealth::descriptor) + "/default";
    ndk::SpAIBinder binder(AServiceManager_checkService(serviceName.c_str()));
    std::shared_ptr<IHealth> health = IHealth::fromBinder(binder);
    if (!health) return power;
    HealthInfo info;
    const ndk::ScopedAStatus status = health->getHealthInfo(&info);
    if (!status.isOk()) {
        LOG(WARNING) << "core_platform_health_unavailable error=" << status.getDescription();
        return power;
    }
    if (info.batteryPresent && info.batteryLevel >= 0 && info.batteryLevel <= 100) {
        power["battery_percent"] = info.batteryLevel;
    }
    const bool charging = info.batteryStatus == BatteryStatus::CHARGING ||
            info.batteryStatus == BatteryStatus::FULL;
    power["charging"] = charging;
    if (info.chargerAcOnline) power["charging_source"] = "ac";
    else if (info.chargerUsbOnline) power["charging_source"] = "usb";
    else if (info.chargerWirelessOnline) power["charging_source"] = "wireless";
    else if (info.chargerDockOnline) power["charging_source"] = "dock";
    if (info.batteryTemperatureTenthsCelsius > -1000 &&
            info.batteryTemperatureTenthsCelsius < 2000) {
        power["battery_temperature_deci_c"] = info.batteryTemperatureTenthsCelsius;
    }
    return power;
}

bool persistedMuted() {
    const auto state = readJson(kAudioStatePath);
    return state && (*state)["muted"].isBool() && (*state)["muted"].asBool();
}

Json::Value mediaSnapshot() {
    Json::Value media(Json::objectValue);
    media["active"] = false;
    media["playing"] = false;
    media["title"] = "";
    media["artist"] = "";
    const auto state = readJson(kMediaStatePath);
    if (!state || !state->isObject() || !(*state)["active"].asBool()) return media;
    media["active"] = true;
    media["playing"] = (*state)["playing"].asBool();
    media["title"] = bounded((*state)["title"].asString());
    media["artist"] = bounded((*state)["artist"].asString());
    return media;
}

Json::Value audioSnapshot(bool* available) {
    Json::Value audio(Json::objectValue);
    audio["volume_percent"] = nullValue();
    audio["muted"] = nullValue();
    audio["media"] = mediaSnapshot();
    int index = 0;
    const android::status_t status = android::AudioSystem::getStreamVolumeIndex(
            AUDIO_STREAM_MUSIC, &index, AUDIO_DEVICE_OUT_DEFAULT);
    *available = status == android::OK;
    if (*available) {
        index = std::clamp(index, 0, kMusicVolumeMaximum);
        audio["volume_percent"] = index * 100 / kMusicVolumeMaximum;
        audio["muted"] = persistedMuted();
    }
    return audio;
}

bool setVolume(int percent) {
    if (percent < 0 || percent > 100) return false;
    const int index = (percent * kMusicVolumeMaximum + 50) / 100;
    return android::AudioSystem::setStreamVolumeIndex(AUDIO_STREAM_MUSIC, index,
            persistedMuted(), AUDIO_DEVICE_OUT_DEFAULT) == android::OK;
}

bool setMuted(bool muted) {
    if (android::AudioSystem::setStreamMute(AUDIO_STREAM_MUSIC, muted) != android::OK) {
        return false;
    }
    Json::Value state(Json::objectValue);
    state["muted"] = muted;
    return writeJsonAtomically(kAudioStatePath, state);
}

std::vector<std::string> onlineInterfaces(bool* wifiPresent) {
    std::vector<std::string> result;
    *wifiPresent = false;
    std::unique_ptr<DIR, decltype(&closedir)> directory(opendir("/sys/class/net"), closedir);
    if (!directory) return result;
    while (dirent* entry = readdir(directory.get())) {
        const std::string name(entry->d_name);
        if (name == "." || name == ".." || name == "lo") continue;
        if (android::base::StartsWith(name, "wlan") || android::base::StartsWith(name, "wifi")) {
            *wifiPresent = true;
        }
        const auto state = readTrimmed("/sys/class/net/" + name + "/operstate");
        if (state && *state == "up") result.push_back(name);
    }
    std::sort(result.begin(), result.end());
    return result;
}

bool socketAddress(const std::string& path, sockaddr_un* address, socklen_t* length) {
    std::memset(address, 0, sizeof(*address));
    address->sun_family = AF_UNIX;
    if (!path.empty() && path[0] == '@') {
        if (path.size() > sizeof(address->sun_path)) return false;
        std::copy(path.begin() + 1, path.end(), address->sun_path + 1);
        *length = static_cast<socklen_t>(sizeof(sa_family_t) + path.size());
    } else {
        if (path.size() + 1 > sizeof(address->sun_path)) return false;
        std::copy(path.begin(), path.end(), address->sun_path);
        *length = static_cast<socklen_t>(sizeof(sa_family_t) + path.size() + 1);
    }
    return true;
}

std::shared_ptr<ISupplicantStaIface> supplicantStaIface() {
    const std::string serviceName = std::string(ISupplicant::descriptor) + "/default";
    ndk::SpAIBinder binder(AServiceManager_checkService(serviceName.c_str()));
    std::shared_ptr<ISupplicant> supplicant = ISupplicant::fromBinder(binder);
    if (!supplicant) return nullptr;
    std::shared_ptr<ISupplicantStaIface> interface;
    ndk::ScopedAStatus status = supplicant->getStaInterface("wlan0", &interface);
    if (!status.isOk() || !interface) {
        status = supplicant->addStaInterface("wlan0", &interface);
    }
    if (!status.isOk()) {
        LOG(WARNING) << "core_platform_supplicant_unavailable error="
                     << status.getDescription();
        return nullptr;
    }
    return interface;
}

std::optional<int32_t> selectedNetworkId() {
    const auto state = readJson(kNetworkStatePath);
    if (!state || !(*state)["selected_id"].isInt()) return std::nullopt;
    const int32_t id = (*state)["selected_id"].asInt();
    return id >= 0 ? std::optional<int32_t>(id) : std::nullopt;
}

bool persistSelectedNetwork(std::optional<int32_t> id) {
    Json::Value state(Json::objectValue);
    state["selected_id"] = id ? Json::Value(*id) : nullValue();
    return writeJsonAtomically(kNetworkStatePath, state);
}

std::vector<SavedNetwork> savedNetworks(const std::shared_ptr<ISupplicantStaIface>& interface,
                                        std::optional<int32_t> connectedId) {
    std::vector<SavedNetwork> result;
    if (!interface) return result;
    std::vector<int32_t> ids;
    if (!interface->listNetworks(&ids).isOk()) return result;
    for (int32_t id : ids) {
        if (result.size() >= 32 || id < 0) break;
        std::shared_ptr<ISupplicantStaNetwork> network;
        if (!interface->getNetwork(id, &network).isOk() || !network) continue;
        std::vector<uint8_t> ssid;
        if (!network->getSsid(&ssid).isOk()) continue;
        const std::string label = bounded(std::string(ssid.begin(), ssid.end()));
        if (label.empty()) continue;
        const std::string identity = std::to_string(id) + ":" + label;
        result.push_back(SavedNetwork{static_cast<int>(id), opaque("network", identity), label,
                                      connectedId && *connectedId == id});
    }
    std::sort(result.begin(), result.end(), [](const SavedNetwork& left, const SavedNetwork& right) {
        return left.label < right.label;
    });
    return result;
}

int signalLevel(int32_t rssi) {
    if (rssi >= -55) return 4;
    if (rssi >= -65) return 3;
    if (rssi >= -75) return 2;
    return 1;
}

Json::Value connectivitySnapshot(std::vector<SavedNetwork>* networks, bool* wifiConnected) {
    bool wifiPresent = false;
    const std::vector<std::string> interfaces = onlineInterfaces(&wifiPresent);
    const bool wifiLinkUp = std::any_of(interfaces.begin(), interfaces.end(), [](const auto& name) {
        return android::base::StartsWith(name, "wlan") || android::base::StartsWith(name, "wifi");
    });
    const std::shared_ptr<ISupplicantStaIface> interface = supplicantStaIface();
    *wifiConnected = wifiLinkUp && interface != nullptr;
    const std::optional<int32_t> selected = selectedNetworkId();
    *networks = savedNetworks(interface, wifiLinkUp ? selected : std::nullopt);
    int level = 0;
    if (interface && wifiLinkUp) {
        std::vector<SignalPollResult> signals;
        if (interface->getSignalPollResults(&signals).isOk() && !signals.empty()) {
            const auto strongest = std::max_element(signals.begin(), signals.end(),
                    [](const SignalPollResult& left, const SignalPollResult& right) {
                        return left.currentRssiDbm < right.currentRssiDbm;
                    });
            level = signalLevel(strongest->currentRssiDbm);
        }
    }
    const bool connected = !interfaces.empty();
    std::string selectedLabel;
    for (const SavedNetwork& network : *networks) {
        if (network.connected) selectedLabel = network.label;
    }

    Json::Value result(Json::objectValue);
    result["wifi_enabled"] = wifiPresent || interface != nullptr;
    result["connected"] = connected;
    // A route or association is not equivalent to Android's validated
    // Internet capability. A future native reachability monitor owns this bit.
    result["validated"] = false;
    result["transport"] = wifiLinkUp ? "wifi" : (connected ? "network" : "");
    result["network_label"] = selectedLabel;
    result["signal_level"] = level > 0 ? Json::Value(level) : nullValue();
    result["online_interfaces"] = Json::arrayValue;
    for (const std::string& interface : interfaces) result["online_interfaces"].append(interface);
    result["wifi_networks"] = Json::arrayValue;
    for (const SavedNetwork& network : *networks) {
        Json::Value item(Json::objectValue);
        item["id"] = network.id;
        item["label"] = network.label;
        item["signal_level"] = network.connected && level > 0 ? level : 1;
        item["saved"] = true;
        item["connected"] = network.connected;
        result["wifi_networks"].append(item);
    }
    return result;
}

std::vector<NativeApp> nativeApps() {
    std::vector<NativeApp> result;
    const auto manifest = readJson(kAppsManifestPath);
    if (!manifest || (*manifest)["version"].asInt() != 1 || !(*manifest)["apps"].isArray()) {
        return result;
    }
    std::set<std::string> identities;
    for (const Json::Value& entry : (*manifest)["apps"]) {
        if (result.size() >= kMaxItems || !entry.isObject()) break;
        const std::string label = bounded(entry["label"].asString());
        const std::string target = entry["target"].asString();
        if (label.empty() || !validInternalTarget(target) || !identities.insert(target).second) {
            continue;
        }
        result.push_back(NativeApp{opaque("app", target), label, target});
    }
    std::sort(result.begin(), result.end(), [](const NativeApp& left, const NativeApp& right) {
        return left.label < right.label;
    });
    return result;
}

Json::Value appsSnapshot(const std::vector<NativeApp>& apps) {
    Json::Value result(Json::objectValue);
    result["compatible"] = Json::arrayValue;
    for (const NativeApp& app : apps) {
        Json::Value item(Json::objectValue);
        item["id"] = app.id;
        item["label"] = app.label;
        result["compatible"].append(item);
    }
    return result;
}

std::string attentionIdentity(const Json::Value& item) {
    return std::to_string(item["occurred_at_ms"].asUInt64()) + ":" +
            item["source"].asString() + ":" + item["kind"].asString() + ":" +
            item["title"].asString();
}

Json::Value normalizedAttentionItems() {
    const auto state = readJson(kAttentionStatePath);
    Json::Value items(Json::arrayValue);
    if (!state || !(*state)["items"].isArray()) return items;
    static const std::set<std::string> kKinds = {
            "general", "message", "call", "alarm", "media", "system", "background"};
    for (const Json::Value& stored : (*state)["items"]) {
        if (items.size() >= kMaxItems || !stored.isObject()) break;
        const std::string kind = stored["kind"].asString();
        const std::string title = bounded(stored["title"].asString());
        if (kKinds.find(kind) == kKinds.end() || title.empty()) continue;
        Json::Value item(Json::objectValue);
        item["id"] = opaque("attention", attentionIdentity(stored));
        item["occurred_at_ms"] = stored["occurred_at_ms"].asUInt64();
        item["source"] = bounded(stored["source"].asString());
        item["kind"] = kind;
        item["urgent"] = stored["urgent"].asBool();
        item["title"] = title;
        item["detail"] = bounded(stored["detail"].asString());
        items.append(item);
    }
    return items;
}

Json::Value attentionSnapshot() {
    Json::Value result(Json::objectValue);
    result["items"] = normalizedAttentionItems();
    uint32_t urgent = 0;
    for (const Json::Value& item : result["items"]) {
        if (item["urgent"].asBool()) ++urgent;
    }
    result["urgent_count"] = urgent;
    return result;
}

bool acknowledgeAttention(const std::string& requestedId) {
    const auto state = readJson(kAttentionStatePath);
    if (!state || !(*state)["items"].isArray()) return false;
    Json::Value retained(Json::arrayValue);
    bool removed = false;
    for (const Json::Value& item : (*state)["items"]) {
        if (!removed && opaque("attention", attentionIdentity(item)) == requestedId) {
            removed = true;
        } else {
            retained.append(item);
        }
    }
    if (!removed) return false;
    Json::Value updated(Json::objectValue);
    updated["version"] = 1;
    updated["items"] = retained;
    return writeJsonAtomically(kAttentionStatePath, updated);
}

bool sendAbstractDatagram(const char* name, std::string_view message) {
    ScopedFd socketFd(socket(AF_UNIX, SOCK_DGRAM | SOCK_CLOEXEC, 0));
    if (socketFd.value < 0) return false;
    sockaddr_un destination{};
    socklen_t length = 0;
    if (!socketAddress(std::string("@") + name, &destination, &length)) return false;
    return sendto(socketFd.value, message.data(), message.size(), MSG_NOSIGNAL,
                   reinterpret_cast<sockaddr*>(&destination), length) ==
            static_cast<ssize_t>(message.size());
}

Json::Value providerSnapshot() {
    bool audioAvailable = false;
    std::vector<SavedNetwork> networks;
    bool wifiConnected = false;
    const Json::Value audio = audioSnapshot(&audioAvailable);
    const Json::Value connectivity = connectivitySnapshot(&networks, &wifiConnected);
    const std::vector<NativeApp> apps = nativeApps();
    const Json::Value attention = attentionSnapshot();

    Json::Value root(Json::objectValue);
    root["abi_version"] = kProviderAbi;
    root["observed_at_ms"] = Json::UInt64(nowMs());
    root["clock"] = emptyClock();
    root["power"] = healthSnapshot();
    root["connectivity"] = connectivity;
    root["audio"] = audio;
    root["apps"] = appsSnapshot(apps);
    root["attention"] = attention;
    root["capabilities"] = Json::arrayValue;
    if (audioAvailable) {
        root["capabilities"].append("audio_set_volume");
        root["capabilities"].append("audio_set_muted");
    }
    if (audio["media"]["active"].asBool()) {
        root["capabilities"].append("media_play_pause");
        root["capabilities"].append("media_next");
        root["capabilities"].append("media_previous");
    }
    if (!networks.empty()) root["capabilities"].append("wifi_connect");
    if (wifiConnected) root["capabilities"].append("wifi_disconnect");
    if (!apps.empty()) root["capabilities"].append("app_launch");
    if (!attention["items"].empty()) root["capabilities"].append("attention_acknowledge");
    return root;
}

bool executeAction(const Json::Value& request) {
    const std::string provider = request["provider"].asString();
    const std::string action = request["action"].asString();
    const Json::Value payload = request["payload"];
    if (provider == "audio" && action == "set_volume" && payload["percent"].isInt()) {
        return setVolume(payload["percent"].asInt());
    }
    if (provider == "audio" && action == "set_muted" && payload["muted"].isBool()) {
        return setMuted(payload["muted"].asBool());
    }
    if (provider == "media" &&
            (action == "play_pause" || action == "next" || action == "previous")) {
        return sendAbstractDatagram("sos_core_media", action);
    }
    if (provider == "network" && action == "disconnect") {
        const std::shared_ptr<ISupplicantStaIface> interface = supplicantStaIface();
        if (!interface || !interface->disconnect().isOk()) return false;
        return persistSelectedNetwork(std::nullopt);
    }
    if (provider == "network" && action == "connect") {
        const std::shared_ptr<ISupplicantStaIface> interface = supplicantStaIface();
        if (!interface) return false;
        const std::string requested = payload["network_id"].asString();
        for (const SavedNetwork& network : savedNetworks(interface, std::nullopt)) {
            if (network.id != requested) continue;
            std::shared_ptr<ISupplicantStaNetwork> selected;
            if (!interface->getNetwork(network.supplicant_id, &selected).isOk() || !selected ||
                    !selected->select().isOk()) {
                return false;
            }
            return persistSelectedNetwork(network.supplicant_id);
        }
        return false;
    }
    if (provider == "apps" && action == "launch") {
        const std::string requested = payload["app_id"].asString();
        for (const NativeApp& app : nativeApps()) {
            if (app.id == requested) return sendAbstractDatagram("sos_core_app_launcher", app.target);
        }
        return false;
    }
    if (provider == "attention" && action == "acknowledge") {
        return acknowledgeAttention(payload["attention_id"].asString());
    }
    return false;
}

bool readFully(int descriptor, void* output, size_t length) {
    size_t offset = 0;
    while (offset < length) {
        const ssize_t count = read(descriptor, static_cast<char*>(output) + offset, length - offset);
        if (count == 0) return false;
        if (count < 0) {
            if (errno == EINTR) continue;
            return false;
        }
        offset += static_cast<size_t>(count);
    }
    return true;
}

bool sendResponse(int client, uint8_t status, const Json::Value& value) {
    const uint32_t magic = htonl(kMagic);
    const std::string encoded = encodeJson(value);
    const uint32_t length = htonl(static_cast<uint32_t>(encoded.size()));
    int sendError = 0;
    const bool sent = sos::core::sendFullyNoSignal(client, &magic, sizeof(magic), &sendError) &&
            sos::core::sendFullyNoSignal(client, &status, sizeof(status), &sendError) &&
            sos::core::sendFullyNoSignal(client, &length, sizeof(length), &sendError) &&
            sos::core::sendFullyNoSignal(client, encoded.data(), encoded.size(), &sendError);
    if (!sent) {
        LOG(WARNING) << "core_platform_response_send_failed error=" << strerror(sendError);
    }
    return sent;
}

void handleClient(int client) {
    ucred credentials{};
    socklen_t credentialLength = sizeof(credentials);
    if (getsockopt(client, SOL_SOCKET, SO_PEERCRED, &credentials, &credentialLength) != 0 ||
            credentials.uid != 1000) {
        LOG(WARNING) << "core_platform_peer_rejected uid=" << credentials.uid;
        return;
    }
    uint32_t wireMagic = 0;
    uint8_t command = 0;
    if (!readFully(client, &wireMagic, sizeof(wireMagic)) ||
            !readFully(client, &command, sizeof(command)) || ntohl(wireMagic) != kMagic) {
        return;
    }
    if (command == 4) {
        sendResponse(client, kResponseOk, providerSnapshot());
        return;
    }
    if (command != 5) return;
    uint32_t wireLength = 0;
    if (!readFully(client, &wireLength, sizeof(wireLength))) return;
    const uint32_t length = ntohl(wireLength);
    if (length == 0 || length > kMaxRequestBytes) return;
    std::string encoded(length, '\0');
    if (!readFully(client, encoded.data(), encoded.size())) return;
    Json::CharReaderBuilder builder;
    Json::Value request;
    std::string error;
    std::istringstream input(encoded);
    if (!Json::parseFromStream(builder, input, &request, &error) || !request.isObject() ||
            !executeAction(request)) {
        Json::Value response(Json::objectValue);
        response["accepted"] = false;
        sendResponse(client, kResponseError, response);
        return;
    }
    Json::Value response(Json::objectValue);
    response["accepted"] = true;
    sendResponse(client, kResponseOk, response);
}

int createServer() {
    ScopedFd server(socket(AF_UNIX, SOCK_STREAM | SOCK_CLOEXEC, 0));
    if (server.value < 0) return -1;
    sockaddr_un address{};
    socklen_t length = 0;
    if (!socketAddress(std::string("@") + kSocketName, &address, &length) ||
            bind(server.value, reinterpret_cast<sockaddr*>(&address), length) != 0 ||
            listen(server.value, 8) != 0) {
        return -1;
    }
    return std::exchange(server.value, -1);
}

}  // namespace

int main() {
    android::base::InitLogging(nullptr, android::base::KernelLogger);
    if (mkdir(kStateDirectory, 0700) != 0 && errno != EEXIST) {
        PLOG(ERROR) << "core_platform_state_directory_failed";
        return 1;
    }
    ScopedFd server(createServer());
    if (server.value < 0) {
        PLOG(ERROR) << "core_platform_socket_failed";
        return 1;
    }
    LOG(INFO) << "core_platform_ready abi=" << kProviderAbi
              << " transport=unix rendered_ui=false";
    while (true) {
        ScopedFd client(accept4(server.value, nullptr, nullptr, SOCK_CLOEXEC));
        if (client.value < 0) {
            if (errno == EINTR) continue;
            PLOG(ERROR) << "core_platform_accept_failed";
            return 1;
        }
        handleClient(client.value);
    }
}
