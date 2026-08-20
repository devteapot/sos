#include <errno.h>
#include <poll.h>
#include <stddef.h>
#include <stdint.h>
#include <string.h>
#include <sys/socket.h>
#include <sys/un.h>
#include <unistd.h>

#include <algorithm>
#include <array>
#include <string_view>

#include "dev_credential_protocol_v1.h"

namespace {

constexpr char kSocketName[] = "sos_core_dev_credential_v1";
constexpr std::array<uint8_t, 4> kMagic{
    SOS_CORE_DEV_V1_MAGIC_0, SOS_CORE_DEV_V1_MAGIC_1, SOS_CORE_DEV_V1_MAGIC_2,
    SOS_CORE_DEV_V1_MAGIC_3};
constexpr uint8_t kVersion = SOS_CORE_DEV_V1_VERSION;
constexpr uint8_t kProbe = SOS_CORE_DEV_V1_OP_PROBE;
constexpr uint8_t kSet = SOS_CORE_DEV_V1_OP_SET;
constexpr uint8_t kClear = SOS_CORE_DEV_V1_OP_CLEAR;
constexpr uint8_t kStatus = SOS_CORE_DEV_V1_OP_STATUS;
constexpr uint8_t kAgentSmoke = SOS_CORE_DEV_V1_OP_AGENT_SMOKE;
constexpr uint8_t kOk = SOS_CORE_DEV_V1_STATUS_OK;
constexpr uint8_t kWrongPeer = SOS_CORE_DEV_V1_STATUS_WRONG_PEER;
constexpr uint8_t kProtocolMismatch = SOS_CORE_DEV_V1_STATUS_PROTOCOL_MISMATCH;
constexpr uint8_t kConfigured = SOS_CORE_DEV_V1_STATUS_CONFIGURED;
constexpr uint8_t kEmpty = SOS_CORE_DEV_V1_STATUS_EMPTY;
constexpr size_t kMinimumCredentialBytes = 20;
constexpr size_t kMaximumCredentialBytes = SOS_CORE_DEV_V1_MAX_PAYLOAD_BYTES;
constexpr std::string_view kOpenRouterPrefix = "sk-or-v1-";
constexpr int kTimeoutMilliseconds = 2000;
constexpr size_t kRequestHeaderBytes = SOS_CORE_DEV_V1_REQUEST_HEADER_BYTES;
constexpr size_t kAckBytes = SOS_CORE_DEV_V1_ACK_BYTES;

static_assert(kMagic == std::array<uint8_t, 4>{'S', 'O', 'S', 'K'});
static_assert(kRequestHeaderBytes == 8);
static_assert(kAckBytes == 6);
static_assert(kMaximumCredentialBytes <= UINT16_MAX);

class SecretBuffer {
public:
  ~SecretBuffer() { memset_explicit(bytes.data(), 0, bytes.size()); }

  std::array<uint8_t, kMaximumCredentialBytes + 1> bytes{};
  size_t size = 0;
};

bool writeAll(int fd, const void *data, size_t size) {
  const auto *cursor = static_cast<const uint8_t *>(data);
  while (size > 0) {
    size_t chunk = size;
#ifdef SOS_CORE_DEV_CREDENTIAL_TEST_MAX_IO_BYTES
    chunk = std::min(
        chunk, static_cast<size_t>(SOS_CORE_DEV_CREDENTIAL_TEST_MAX_IO_BYTES));
#endif
    const ssize_t written = write(fd, cursor, chunk);
    if (written < 0 && errno == EINTR)
      continue;
    if (written <= 0)
      return false;
    cursor += written;
    size -= static_cast<size_t>(written);
  }
  return true;
}

bool readAll(int fd, void *data, size_t size) {
  auto *cursor = static_cast<uint8_t *>(data);
  while (size > 0) {
    size_t chunk = size;
#ifdef SOS_CORE_DEV_CREDENTIAL_TEST_MAX_IO_BYTES
    chunk = std::min(
        chunk, static_cast<size_t>(SOS_CORE_DEV_CREDENTIAL_TEST_MAX_IO_BYTES));
#endif
    const ssize_t received = read(fd, cursor, chunk);
    if (received < 0 && errno == EINTR)
      continue;
    if (received <= 0)
      return false;
    cursor += received;
    size -= static_cast<size_t>(received);
  }
  return true;
}

bool waitFor(int fd, short events) {
  pollfd descriptor{fd, events, 0};
  int result;
  do {
    result = poll(&descriptor, 1, kTimeoutMilliseconds);
  } while (result < 0 && errno == EINTR);
  return result == 1 && (descriptor.revents & events) != 0 &&
         (descriptor.revents & (POLLERR | POLLNVAL)) == 0;
}

bool readCredential(SecretBuffer *secret) {
  for (;;) {
    uint8_t byte = 0;
    const ssize_t received = read(STDIN_FILENO, &byte, 1);
    if (received < 0 && errno == EINTR)
      continue;
    if (received == 0 || byte == '\n')
      break;
    if (secret->size == secret->bytes.size())
      return false;
    secret->bytes[secret->size++] = byte;
  }
  if (secret->size > 0 && secret->bytes[secret->size - 1] == '\r')
    --secret->size;
  if (secret->size < kMinimumCredentialBytes ||
      secret->size > kMaximumCredentialBytes ||
      secret->size < kOpenRouterPrefix.size() ||
      memcmp(secret->bytes.data(), kOpenRouterPrefix.data(),
             kOpenRouterPrefix.size()) != 0) {
    return false;
  }
  for (size_t index = 0; index < secret->size; ++index) {
    if (secret->bytes[index] < 0x21 || secret->bytes[index] > 0x7e)
      return false;
  }
  return true;
}

int connectEndpoint() {
  const int fd = socket(AF_UNIX, SOCK_STREAM | SOCK_CLOEXEC, 0);
  if (fd < 0)
    return -1;
  timeval timeout{2, 0};
  if (setsockopt(fd, SOL_SOCKET, SO_RCVTIMEO, &timeout, sizeof(timeout)) != 0 ||
      setsockopt(fd, SOL_SOCKET, SO_SNDTIMEO, &timeout, sizeof(timeout)) != 0) {
    close(fd);
    return -1;
  }
  sockaddr_un address{};
  address.sun_family = AF_UNIX;
  address.sun_path[0] = '\0';
  static_assert(sizeof(kSocketName) < sizeof(address.sun_path));
  memcpy(address.sun_path + 1, kSocketName, sizeof(kSocketName) - 1);
  const socklen_t addressLength =
      offsetof(sockaddr_un, sun_path) + sizeof(kSocketName);
  if (connect(fd, reinterpret_cast<const sockaddr *>(&address),
              addressLength) != 0) {
    close(fd);
    return -1;
  }
  return fd;
}

enum class ExchangeResult {
  kOk,
  kEndpointUnavailable,
  kShortIo,
  kWrongPeer,
  kBadMagic,
  kBadVersion,
  kProtocolMismatchStatus,
  kBadStatus,
  kRequestRejected,
  kConfigured,
  kEmpty,
};

ExchangeResult exchangeOnFd(int fd, uint8_t operation,
                            const SecretBuffer &secret) {
  std::array<uint8_t, kRequestHeaderBytes + kMaximumCredentialBytes> request{};
  std::copy(kMagic.begin(), kMagic.end(), request.begin());
  request[4] = kVersion;
  request[5] = operation;
  request[6] = static_cast<uint8_t>((secret.size >> 8) & 0xff);
  request[7] = static_cast<uint8_t>(secret.size & 0xff);
  if (secret.size > 0) {
    memcpy(request.data() + kRequestHeaderBytes, secret.bytes.data(),
           secret.size);
  }
  const size_t requestBytes = kRequestHeaderBytes + secret.size;
  const bool sent = waitFor(fd, POLLOUT) &&
                    writeAll(fd, request.data(), requestBytes) &&
                    shutdown(fd, SHUT_WR) == 0;
  memset_explicit(request.data(), 0, request.size());
  std::array<uint8_t, kAckBytes> response{};
  const bool received = sent && waitFor(fd, POLLIN) &&
                        readAll(fd, response.data(), response.size());
  if (!received)
    return ExchangeResult::kShortIo;
  if (memcmp(response.data(), kMagic.data(), kMagic.size()) != 0)
    return ExchangeResult::kBadMagic;
  if (response[4] != kVersion)
    return ExchangeResult::kBadVersion;
  if (response[5] == kOk)
    return ExchangeResult::kOk;
  if (response[5] == kWrongPeer)
    return ExchangeResult::kWrongPeer;
  if (response[5] == kProtocolMismatch)
    return ExchangeResult::kProtocolMismatchStatus;
  if (response[5] == SOS_CORE_DEV_V1_STATUS_REJECTED)
    return ExchangeResult::kRequestRejected;
  if (response[5] == kConfigured)
    return ExchangeResult::kConfigured;
  if (response[5] == kEmpty)
    return ExchangeResult::kEmpty;
  return ExchangeResult::kBadStatus;
}

[[maybe_unused]] ExchangeResult exchange(uint8_t operation,
                                         const SecretBuffer &secret) {
  const int fd = connectEndpoint();
  if (fd < 0)
    return ExchangeResult::kEndpointUnavailable;
  const ExchangeResult result = exchangeOnFd(fd, operation, secret);
  close(fd);
  return result;
}

bool writeMessage(int fd, std::string_view message) {
  return writeAll(fd, message.data(), message.size());
}

int fail(std::string_view category) {
  constexpr std::string_view kPrefix =
      "error: Core development credential request failed (";
  constexpr std::string_view kSuffix = ")\n";
  const bool prefixWritten = writeMessage(STDERR_FILENO, kPrefix);
  const bool categoryWritten =
      prefixWritten && writeMessage(STDERR_FILENO, category);
  if (categoryWritten)
    (void)writeMessage(STDERR_FILENO, kSuffix);
  return 1;
}

} // namespace

int runClient(int argc, char **argv,
              ExchangeResult (*exchangeRequest)(uint8_t,
                                                const SecretBuffer &)) {
  if (argc != 2)
    return fail("usage");
  SecretBuffer secret;
  uint8_t operation = 0;
  std::string_view confirmation;
  if (strcmp(argv[1], "probe") == 0) {
    operation = kProbe;
    confirmation = "core_dev_credential=READY\n";
  } else if (strcmp(argv[1], "set") == 0) {
    operation = kSet;
    confirmation = "core_dev_credential=SET\n";
    if (!readCredential(&secret))
      return fail("credential_format");
  } else if (strcmp(argv[1], "clear") == 0) {
    operation = kClear;
    confirmation = "core_dev_credential=CLEARED\n";
  } else if (strcmp(argv[1], "status") == 0) {
    operation = kStatus;
  } else if (strcmp(argv[1], "agent-smoke") == 0) {
    operation = kAgentSmoke;
    confirmation = "core_dev_agent_smoke=SUBMITTED\n";
  } else {
    return fail("usage");
  }
  const ExchangeResult result = exchangeRequest(operation, secret);
  if (operation == kStatus) {
    if (result == ExchangeResult::kConfigured) {
      confirmation = "core_dev_credential=CONFIGURED\n";
    } else if (result == ExchangeResult::kEmpty) {
      confirmation = "core_dev_credential=EMPTY\n";
    }
  }
  if ((operation != kStatus && result == ExchangeResult::kOk) ||
      (operation == kStatus &&
       (result == ExchangeResult::kConfigured ||
        result == ExchangeResult::kEmpty))) {
    if (!writeMessage(STDOUT_FILENO, confirmation))
      return fail("stdout_io");
    return 0;
  }
  switch (result) {
  case ExchangeResult::kOk:
    return fail("unexpected_status");
  case ExchangeResult::kEndpointUnavailable:
    return fail("endpoint_unavailable");
  case ExchangeResult::kShortIo:
    return fail("short_io");
  case ExchangeResult::kWrongPeer:
    return fail("wrong_peer");
  case ExchangeResult::kBadMagic:
    return fail("bad_magic");
  case ExchangeResult::kBadVersion:
    return fail("bad_version");
  case ExchangeResult::kProtocolMismatchStatus:
    return fail("protocol_mismatch_status");
  case ExchangeResult::kBadStatus:
    return fail("bad_status");
  case ExchangeResult::kRequestRejected:
    return fail("request_rejected");
  case ExchangeResult::kConfigured:
  case ExchangeResult::kEmpty:
    return fail("unexpected_status");
  }
  return fail("unknown");
}

#ifndef SOS_CORE_DEV_CREDENTIAL_NO_MAIN
int main(int argc, char **argv) { return runClient(argc, argv, exchange); }
#endif
