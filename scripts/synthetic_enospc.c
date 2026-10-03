/* Test-process-only Linux ENOSPC injection. Never fills a filesystem.
 * The driver owns a fresh marked root and passes these variables only to its
 * child. A control file arms a path prefix plus the permitted byte offset.
 * Calls outside that root, stdout/stderr and unarmed writes are unchanged.
 */
#define _GNU_SOURCE
#include <errno.h>
#include <fcntl.h>
#include <limits.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include <sys/syscall.h>
#include <sys/types.h>
#include <unistd.h>

static ssize_t read_control(const char *path, char *buffer, size_t capacity) {
    int fd = syscall(SYS_openat, AT_FDCWD, path, O_RDONLY | O_CLOEXEC | O_NOFOLLOW, 0);
    if (fd < 0) return -1;
    ssize_t length = syscall(SYS_read, fd, buffer, capacity - 1);
    syscall(SYS_close, fd);
    if (length >= 0) buffer[length] = '\0';
    return length;
}

static size_t permitted(int fd, size_t count, off_t offset, int *blocked) {
    const char *root = getenv("MANGAMONITOR_FAULT_ROOT");
    const char *control = getenv("MANGAMONITOR_FAULT_CONTROL");
    const char *trace = getenv("MANGAMONITOR_FAULT_TRACE");
    *blocked = 0;
    if (!count || !root || !control || !trace || strlen(root) < 16) return count;
    if (strncmp(root, "/workspace/", 11) && strncmp(root, "/tmp/", 5)) return count;
    char marker_path[PATH_MAX], marker[64], instruction[PATH_MAX + 64];
    if (snprintf(marker_path, sizeof(marker_path), "%s/.synthetic-fault-root", root) >= (int)sizeof(marker_path)) return count;
    if (read_control(marker_path, marker, sizeof(marker)) < 0 || strcmp(marker, "isolated synthetic ENOSPC probe\n")) return count;
    if (read_control(control, instruction, sizeof(instruction)) < 0) return count;
    char *newline = strchr(instruction, '\n');
    if (!newline) return count;
    *newline++ = '\0';
    const size_t root_length = strlen(root), prefix_length = strlen(instruction);
    if (prefix_length <= root_length || strncmp(instruction, root, root_length) || instruction[root_length] != '/') return count;
    char *end = NULL;
    errno = 0;
    unsigned long long limit = strtoull(newline, &end, 10);
    if (errno || end == newline || (*end != '\n' && *end != '\0') || limit > 1048576) return count;
    char fd_path[64], target[PATH_MAX];
    snprintf(fd_path, sizeof(fd_path), "/proc/self/fd/%d", fd);
    ssize_t length = syscall(SYS_readlinkat, AT_FDCWD, fd_path, target, sizeof(target) - 1);
    if (length < 0) return count;
    target[length] = '\0';
    if (strncmp(target, instruction, prefix_length)) return count;
    if (offset < 0) offset = syscall(SYS_lseek, fd, 0, SEEK_CUR);
    if (offset < 0) return count;
    if ((unsigned long long)offset < limit) {
        const size_t remaining = (size_t)(limit - (unsigned long long)offset);
        return remaining < count ? remaining : count;
    }
    *blocked = 1;
    int log_fd = syscall(SYS_openat, AT_FDCWD, trace, O_WRONLY | O_CREAT | O_APPEND | O_CLOEXEC | O_NOFOLLOW, 0600);
    if (log_fd >= 0) {
        syscall(SYS_write, log_fd, target, (size_t)length);
        syscall(SYS_write, log_fd, "\n", 1);
        syscall(SYS_close, log_fd);
    }
    return 0;
}

ssize_t write(int fd, const void *buffer, size_t count) {
    int blocked;
    size_t allowed = permitted(fd, count, -1, &blocked);
    if (blocked) { errno = ENOSPC; return -1; }
    return syscall(SYS_write, fd, buffer, allowed);
}

ssize_t pwrite(int fd, const void *buffer, size_t count, off_t offset) {
    int blocked;
    size_t allowed = permitted(fd, count, offset, &blocked);
    if (blocked) { errno = ENOSPC; return -1; }
    return syscall(SYS_pwrite64, fd, buffer, allowed, offset);
}
