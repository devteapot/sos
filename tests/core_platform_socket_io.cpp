#include "aosp/device/sos/a33x/core/socket_io.h"

#include <signal.h>
#include <sys/socket.h>
#include <sys/wait.h>
#include <unistd.h>

#include <algorithm>
#include <cerrno>
#include <cstdio>
#include <cstdlib>
#include <cstring>
#include <thread>
#include <vector>

namespace {

void require(bool condition, const char* message) {
    if (!condition) {
        std::fprintf(stderr, "core_platform_socket_io_test_failed reason=%s\n", message);
        std::exit(1);
    }
}

void oldPlainWriteTerminatesOnClosedPeer() {
    int sockets[2];
    require(socketpair(AF_UNIX, SOCK_STREAM | SOCK_CLOEXEC, 0, sockets) == 0, "control socketpair");
    close(sockets[1]);
    const pid_t child = fork();
    require(child >= 0, "control fork");
    if (child == 0) {
        signal(SIGPIPE, SIG_DFL);
        const char byte = 'x';
        write(sockets[0], &byte, sizeof(byte));
        _exit(0);
    }
    close(sockets[0]);
    int status = 0;
    require(waitpid(child, &status, 0) == child, "control wait");
    require(WIFSIGNALED(status) && WTERMSIG(status) == SIGPIPE,
            "plain write must reproduce SIGPIPE");
}

void protectedSendSurvivesClosedPeer() {
    int sockets[2];
    require(socketpair(AF_UNIX, SOCK_STREAM | SOCK_CLOEXEC, 0, sockets) == 0,
            "protected socketpair");
    close(sockets[1]);
    const pid_t child = fork();
    require(child >= 0, "protected fork");
    if (child == 0) {
        signal(SIGPIPE, SIG_DFL);
        const char byte = 'x';
        int error = 0;
        const bool sent = sos::core::sendFullyNoSignal(sockets[0], &byte, sizeof(byte), &error);
        _exit(!sent && (error == EPIPE || error == ECONNRESET) ? 0 : 2);
    }
    close(sockets[0]);
    int status = 0;
    require(waitpid(child, &status, 0) == child, "protected wait");
    require(WIFEXITED(status) && WEXITSTATUS(status) == 0,
            "MSG_NOSIGNAL send must surface peer close");
}

void retriesInterruptedAndPartialSends() {
    const char input[] = "complete-response";
    std::vector<char> delivered;
    size_t calls = 0;
    int error = -1;
    const bool sent = sos::core::detail::sendFullyNoSignalUsing(
            7, input, sizeof(input) - 1, &error,
            [&](int descriptor, const void* bytes, size_t length, int flags) -> ssize_t {
                require(descriptor == 7, "scripted descriptor");
                require((flags & MSG_NOSIGNAL) != 0, "scripted MSG_NOSIGNAL");
                ++calls;
                if (calls == 2) {
                    errno = EINTR;
                    return -1;
                }
                const size_t count = std::min<size_t>(3, length);
                const auto* start = static_cast<const char*>(bytes);
                delivered.insert(delivered.end(), start, start + count);
                return static_cast<ssize_t>(count);
            });
    require(sent && error == 0, "scripted send result");
    require(delivered == std::vector<char>(input, input + sizeof(input) - 1),
            "scripted complete payload");
    require(calls > 2, "scripted retries");
}

void sendsCompleteLargeResponse() {
    int sockets[2];
    require(socketpair(AF_UNIX, SOCK_STREAM | SOCK_CLOEXEC, 0, sockets) == 0, "large socketpair");
    std::vector<char> input(1024 * 1024);
    for (size_t index = 0; index < input.size(); ++index) {
        input[index] = static_cast<char>(index % 251);
    }
    std::vector<char> output(input.size());
    std::thread reader([&] {
        size_t offset = 0;
        while (offset < output.size()) {
            const ssize_t count = read(sockets[1], output.data() + offset, output.size() - offset);
            require(count > 0, "large read");
            offset += static_cast<size_t>(count);
        }
    });
    int error = 0;
    require(sos::core::sendFullyNoSignal(sockets[0], input.data(), input.size(), &error),
            "large send");
    shutdown(sockets[0], SHUT_WR);
    reader.join();
    require(output == input, "large response content");
    close(sockets[0]);
    close(sockets[1]);
}

void survivesPeerCloseDuringReplyAndServesNextPeer() {
    int broken[2];
    require(socketpair(AF_UNIX, SOCK_STREAM | SOCK_CLOEXEC, 0, broken) == 0, "broken socketpair");
    int sendBuffer = 4096;
    require(setsockopt(broken[0], SOL_SOCKET, SO_SNDBUF, &sendBuffer, sizeof(sendBuffer)) == 0,
            "small send buffer");
    const pid_t child = fork();
    require(child >= 0, "during-reply fork");
    if (child == 0) {
        signal(SIGPIPE, SIG_DFL);
        close(broken[1]);
        std::vector<char> response(1024 * 1024, 'r');
        int error = 0;
        const bool sent =
                sos::core::sendFullyNoSignal(broken[0], response.data(), response.size(), &error);
        _exit(!sent && (error == EPIPE || error == ECONNRESET) ? 0 : 3);
    }
    close(broken[0]);
    char prefix[1024];
    require(read(broken[1], prefix, sizeof(prefix)) > 0, "during-reply prefix");
    close(broken[1]);
    int status = 0;
    require(waitpid(child, &status, 0) == child, "during-reply wait");
    require(WIFEXITED(status) && WEXITSTATUS(status) == 0,
            "peer close during reply must not terminate sender");

    int healthy[2];
    require(socketpair(AF_UNIX, SOCK_STREAM | SOCK_CLOEXEC, 0, healthy) == 0,
            "next-peer socketpair");
    const char response[] = "next-response";
    int error = 0;
    require(sos::core::sendFullyNoSignal(healthy[0], response, sizeof(response), &error),
            "next-peer send");
    char received[sizeof(response)]{};
    require(read(healthy[1], received, sizeof(received)) == static_cast<ssize_t>(sizeof(received)),
            "next-peer read");
    require(std::memcmp(response, received, sizeof(response)) == 0, "next-peer content");
    close(healthy[0]);
    close(healthy[1]);
}

}  // namespace

int main() {
    oldPlainWriteTerminatesOnClosedPeer();
    protectedSendSurvivesClosedPeer();
    retriesInterruptedAndPartialSends();
    sendsCompleteLargeResponse();
    survivesPeerCloseDuringReplyAndServesNextPeer();
    std::puts("core_platform_socket_io_test status=PASS");
    return 0;
}
