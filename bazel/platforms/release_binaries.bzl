"""Rules for building release binaries across supported target platforms."""

load("@rules_platform//platform_data:defs.bzl", "platform_data")

PLATFORMS = [
    "linux_arm64_musl",
    "linux_amd64_musl",
    "macos_amd64",
    "macos_arm64",
    "windows_amd64",
    "windows_arm64",
]

PLATFORM_TARGETS = {
    "linux_arm64_musl": "aarch64-unknown-linux-musl",
    "linux_amd64_musl": "x86_64-unknown-linux-musl",
    "macos_amd64": "x86_64-apple-darwin",
    "macos_arm64": "aarch64-apple-darwin",
    "windows_amd64": "x86_64-pc-windows-msvc",
    "windows_arm64": "aarch64-pc-windows-msvc",
}

def multiplatform_binaries(
        name,
        target = None,
        filegroup_name = "release_binaries",
        platforms = PLATFORMS):
    binary_target = target or name
    for platform in platforms:
        platform_data(
            name = name + "_" + platform,
            platform = "@llvm//platforms:" + platform,
            target = binary_target,
            tags = ["manual"],
        )

    if filegroup_name:
        native.filegroup(
            name = filegroup_name,
            srcs = [name + "_" + platform for platform in platforms],
            tags = ["manual"],
        )
