"""Bazel runtime-layout seed targets for Go SDK integration tests."""

load("//bazel/platforms:release_binaries.bzl", "PLATFORMS", "PLATFORM_TARGETS")

def go_sdk_runtime_layouts(
        name,
        binary,
        code_mode_host_binary,
        platforms = PLATFORMS):
    """Creates one Go SDK runtime layout seed per release platform."""

    targets = []
    for platform in platforms:
        target = PLATFORM_TARGETS[platform]
        target_name = name + "_" + platform
        code_mode_host_name = "codex-code-mode-host"
        if platform.startswith("windows_"):
            code_mode_host_name = "codex-code-mode-host.exe"
        _go_sdk_runtime_layout(
            name = target_name,
            code_mode_host = ":" + code_mode_host_binary + "_" + platform,
            code_mode_host_name = code_mode_host_name,
            codex = ":" + binary + "_" + platform,
            entrypoint_name = "codex.exe" if platform.startswith("windows_") else "codex",
            tags = ["manual"],
            target_triple = target,
        )
        targets.append(target_name)

    native.filegroup(
        name = name,
        srcs = targets,
        tags = ["manual"],
    )

def _go_sdk_runtime_layout_impl(ctx):
    entrypoint = ctx.actions.declare_file(
        ctx.label.name + "/bin/" + ctx.attr.entrypoint_name,
    )
    code_mode_host = ctx.actions.declare_file(
        ctx.label.name + "/bin/" + ctx.attr.code_mode_host_name,
    )
    metadata = ctx.actions.declare_file(ctx.label.name + "/codex-package.json")

    ctx.actions.symlink(
        output = entrypoint,
        target_file = ctx.executable.codex,
        is_executable = True,
    )
    ctx.actions.symlink(
        output = code_mode_host,
        target_file = ctx.executable.code_mode_host,
        is_executable = True,
    )
    ctx.actions.write(
        output = metadata,
        content = _metadata_content(
            ctx.attr.target_triple,
            ctx.attr.entrypoint_name,
        ),
    )

    return [
        DefaultInfo(
            files = depset([entrypoint, code_mode_host, metadata]),
            runfiles = ctx.runfiles(files = [entrypoint, code_mode_host, metadata]),
        ),
    ]

def _metadata_content(target_triple, entrypoint_name):
    return """{
  "entrypoint": "bin/%s",
  "layoutVersion": 1,
  "pathDir": "codex-path",
  "resourcesDir": "codex-resources",
  "target": "%s",
  "variant": "codex",
  "version": "0.0.0"
}
""" % (entrypoint_name, target_triple)

_go_sdk_runtime_layout = rule(
    implementation = _go_sdk_runtime_layout_impl,
    attrs = {
        "code_mode_host": attr.label(
            executable = True,
            mandatory = True,
            cfg = "target",
        ),
        "code_mode_host_name": attr.string(mandatory = True),
        "codex": attr.label(
            executable = True,
            mandatory = True,
            cfg = "target",
        ),
        "entrypoint_name": attr.string(mandatory = True),
        "target_triple": attr.string(mandatory = True),
    },
)
