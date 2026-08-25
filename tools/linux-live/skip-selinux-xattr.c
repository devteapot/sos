#define _GNU_SOURCE

#include <dlfcn.h>
#include <errno.h>
#include <stdio.h>
#include <stddef.h>
#include <stdlib.h>
#include <string.h>
#include <sys/types.h>
#include <sys/xattr.h>
#include <unistd.h>

typedef int (*lsetxattr_fn)(const char *, const char *, const void *, size_t,
                           int);
typedef int (*lchown_fn)(const char *, uid_t, gid_t);
typedef ssize_t (*lgetxattr_fn)(const char *, const char *, void *, size_t);
typedef ssize_t (*llistxattr_fn)(const char *, char *, size_t);

static lsetxattr_fn real_lsetxattr;
static lchown_fn real_lchown;
static lgetxattr_fn real_lgetxattr;
static llistxattr_fn real_llistxattr;

static void *rootless_xattr_symbol(const char *name) {
  void *symbol = dlsym(RTLD_NEXT, name);

  if (symbol == NULL) {
    dprintf(STDERR_FILENO, "rootless xattr compatibility: missing %s\n", name);
    _exit(125);
  }
  return symbol;
}

static lsetxattr_fn rootless_real_lsetxattr(void) {
  if (real_lsetxattr == NULL) {
    *(void **)(&real_lsetxattr) = rootless_xattr_symbol("lsetxattr");
  }
  return real_lsetxattr;
}

static lgetxattr_fn rootless_real_lgetxattr(void) {
  if (real_lgetxattr == NULL) {
    *(void **)(&real_lgetxattr) = rootless_xattr_symbol("lgetxattr");
  }
  return real_lgetxattr;
}

static llistxattr_fn rootless_real_llistxattr(void) {
  if (real_llistxattr == NULL) {
    *(void **)(&real_llistxattr) = rootless_xattr_symbol("llistxattr");
  }
  return real_llistxattr;
}

int lsetxattr(const char *path, const char *name, const void *value, size_t size,
              int flags) {
  if (name != NULL && strcmp(name, "security.selinux") == 0) {
    return 0;
  }
  return rootless_real_lsetxattr()(path, name, value, size, flags);
}

ssize_t lgetxattr(const char *path, const char *name, void *value, size_t size) {
  if (name != NULL && strcmp(name, "security.selinux") == 0) {
    errno = ENODATA;
    return -1;
  }
  return rootless_real_lgetxattr()(path, name, value, size);
}

ssize_t llistxattr(const char *path, char *list, size_t size) {
  char *source;
  ssize_t source_size;
  size_t filtered_size = 0;
  char *name;

  source_size = rootless_real_llistxattr()(path, NULL, 0);
  if (source_size <= 0) {
    return source_size;
  }
  source = malloc((size_t)source_size);
  if (source == NULL) {
    errno = ENOMEM;
    return -1;
  }
  source_size = rootless_real_llistxattr()(path, source, (size_t)source_size);
  if (source_size < 0) {
    free(source);
    return -1;
  }
  for (name = source; name < source + source_size; name += strlen(name) + 1) {
    size_t name_size = strlen(name) + 1;

    if (strcmp(name, "security.selinux") == 0) {
      continue;
    }
    if (list != NULL) {
      if (filtered_size + name_size > size) {
        free(source);
        errno = ERANGE;
        return -1;
      }
      memcpy(list + filtered_size, name, name_size);
    }
    filtered_size += name_size;
  }
  free(source);
  return (ssize_t)filtered_size;
}

int lchown(const char *path, uid_t owner, gid_t group) {
  unsigned char capability[64];
  ssize_t capability_size;
  int result;

  capability_size = rootless_real_lgetxattr()(path, "security.capability",
                                              capability, sizeof(capability));
  if (capability_size < 0 && errno != ENODATA && errno != EOPNOTSUPP) {
    dprintf(STDERR_FILENO,
            "rootless xattr compatibility: cannot read capability for %s: %s\n",
            path, strerror(errno));
    _exit(125);
  }
  if (real_lchown == NULL) {
    *(void **)(&real_lchown) = rootless_xattr_symbol("lchown");
  }
  result = real_lchown(path, owner, group);
  if (result != 0 || capability_size <= 0) {
    return result;
  }
  if (rootless_real_lsetxattr()(path, "security.capability", capability,
                                (size_t)capability_size, 0) != 0) {
    dprintf(STDERR_FILENO,
            "rootless xattr compatibility: cannot restore capability for %s: %s\n",
            path, strerror(errno));
    _exit(125);
  }
  return 0;
}
