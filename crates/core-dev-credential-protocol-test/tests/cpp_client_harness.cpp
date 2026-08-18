#define SOS_CORE_DEV_CREDENTIAL_NO_MAIN
#define SOS_CORE_DEV_CREDENTIAL_TEST_MAX_IO_BYTES 1
#include <fcntl.h>
#include "../../../aosp/device/sos/a33x/core/dev_credential_client.cpp"

namespace {

const char *gSocketPath = nullptr;

int connectTestEndpoint() {
  const int fd = socket(AF_UNIX, SOCK_STREAM | SOCK_CLOEXEC, 0);
  if (fd < 0)
    return -1;
  sockaddr_un address{};
  address.sun_family = AF_UNIX;
  const size_t pathBytes = strlen(gSocketPath);
  if (pathBytes == 0 || pathBytes >= sizeof(address.sun_path)) {
    close(fd);
    return -1;
  }
  memcpy(address.sun_path, gSocketPath, pathBytes + 1);
  const socklen_t addressLength =
      offsetof(sockaddr_un, sun_path) + pathBytes + 1;
  if (connect(fd, reinterpret_cast<const sockaddr *>(&address),
              addressLength) != 0) {
    close(fd);
    return -1;
  }
  return fd;
}

ExchangeResult exchangeTest(uint8_t operation, const SecretBuffer &secret) {
  const int fd = connectTestEndpoint();
  if (fd < 0)
    return ExchangeResult::kEndpointUnavailable;
  const ExchangeResult result = exchangeOnFd(fd, operation, secret);
  close(fd);
  return result;
}

} // namespace

int main(int argc, char **argv) {
  if (argc != 3)
    return 64;
  gSocketPath = argv[1];
  const bool closeStdout = strcmp(argv[2], "probe-closed-stdout") == 0;
  char probe[] = "probe";
  char *clientArgv[] = {argv[0], closeStdout ? probe : argv[2]};
  if (closeStdout) {
    const int readOnly = open("/dev/null", O_RDONLY | O_CLOEXEC);
    if (readOnly < 0 || dup2(readOnly, STDOUT_FILENO) < 0)
      return 65;
    close(readOnly);
  }
  return runClient(2, clientArgv, exchangeTest);
}
