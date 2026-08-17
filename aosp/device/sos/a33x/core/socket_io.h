#pragma once

#include <cerrno>
#include <cstddef>
#include <sys/socket.h>

namespace sos::core {

namespace detail {

template <typename Send>
bool sendFullyNoSignalUsing(int descriptor, const void* input, size_t length, int* error,
                            Send&& send) {
    if (error != nullptr) *error = 0;
    size_t offset = 0;
    while (offset < length) {
        const ssize_t count = send(descriptor, static_cast<const char*>(input) + offset,
                                   length - offset, MSG_NOSIGNAL);
        if (count < 0) {
            if (errno == EINTR) continue;
            if (error != nullptr) *error = errno;
            return false;
        }
        if (count == 0) {
            if (error != nullptr) *error = EPIPE;
            return false;
        }
        offset += static_cast<size_t>(count);
    }
    return true;
}

}  // namespace detail

inline bool sendFullyNoSignal(int descriptor, const void* input, size_t length, int* error) {
    return detail::sendFullyNoSignalUsing(
            descriptor, input, length, error,
            [](int socket, const void* bytes, size_t size, int flags) {
                return ::send(socket, bytes, size, flags);
            });
}

}  // namespace sos::core
