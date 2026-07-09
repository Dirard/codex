"""Bazel runtime-layout seed targets for Go SDK integration tests."""

load("//bazel/platforms:release_binaries.bzl", "PLATFORMS", "PLATFORM_TARGETS")

def go_sdk_runtime_layouts(name, binary, platforms = PLATFORMS):
    """Creates one Go SDK runtime layout seed per release platform."""

    targets = []
    for platform in platforms:
        target = PLATFORM_TARGETS[platform]
        target_name = name + "_" + platform
        _go_sdk_runtime_layout(
            name = target_name,
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
    metadata = ctx.actions.declare_file(ctx.label.name + "/codex-package.json")

    ctx.actions.symlink(
        output = entrypoint,
        target_file = ctx.executable.codex,
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
            files = depset([entrypoint, metadata]),
            runfiles = ctx.runfiles(files = [entrypoint, metadata]),
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
        "codex": attr.label(
            executable = True,
            mandatory = True,
            cfg = "target",
        ),
        "entrypoint_name": attr.string(mandatory = True),
        "target_triple": attr.string(mandatory = True),
    },
)
