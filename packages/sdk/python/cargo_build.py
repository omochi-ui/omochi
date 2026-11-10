"""Keep Cargo's compiler flags when selecting the native adapters' CRT."""

import json
import os
import re
import subprocess
import tomllib
from pathlib import Path


def _matches_cfg(expression: str, cfg: set[str]) -> bool:
    tokens = iter(re.findall(r'\w+|"(?:\\.|[^"\\])*"|[(),=]', expression))
    token = next(tokens, None)

    def parse() -> bool:
        nonlocal token
        name, token = token, next(tokens, None)
        if token == "=":
            value = next(tokens)
            token = next(tokens, None)
            return f"{name}={json.dumps(json.loads(value))}" in cfg
        if token != "(":
            return name in cfg
        token = next(tokens)
        values = []
        while token != ")":
            if token is None:
                raise ValueError(f"Invalid Cargo target cfg: {expression}")
            values.append(parse())
            if token == ",":
                token = next(tokens)
        token = next(tokens, None)
        if name in ("cfg", "all"):
            return all(values)
        if name == "any":
            return any(values)
        if name == "not" and len(values) == 1:
            return not values[0]
        raise ValueError(f"Invalid Cargo target cfg: {expression}")

    return parse()


def _flags(value: str | list[str]) -> list[str]:
    return value.split() if isinstance(value, str) else value


def native_build_environment(
    cwd: Path, *, target: str | None = None, release: bool = True
) -> dict[str, str]:
    """Apply CRT policy before Cargo probes cdylib support, retaining active flags."""
    env = dict(os.environ)
    # Cargo probes the working directory's ancestors, then CARGO_HOME. Read only
    # build.target and rustflags; all other configuration stays with Cargo.
    directories = [cwd.resolve(), *cwd.resolve().parents]
    locations = [directory / ".cargo" for directory in directories]
    locations.append(Path(env.get("CARGO_HOME", Path.home() / ".cargo")))
    configs = []
    seen = set()
    for directory in locations:
        config = directory / "config"
        if not config.is_file():
            config = directory / "config.toml"
        if config.is_file() and config.resolve() not in seen:
            seen.add(config.resolve())
            with config.open("rb") as source:
                configs.append(tomllib.load(source))
    selected = target or env.get("CARGO_BUILD_TARGET")
    if not selected:
        for config in configs:
            value = config.get("build", {}).get("target")
            if value is not None:
                if selected is None:
                    selected = value
                elif isinstance(selected, list) and isinstance(value, list):
                    selected = value + selected
                else:
                    break
    if isinstance(selected, list):
        if len(selected) != 1:
            raise ValueError("Build one Cargo target at a time with --target")
        selected = selected[0]
    if not selected or selected == "host-tuple":
        selected = next(
            line.split(": ", 1)[1]
            for line in subprocess.check_output(
                ["rustc", "-vV"], text=True
            ).splitlines()
            if line.startswith("host: ")
        )
    feature = (
        "-crt-static"
        if "musl" in selected
        else "+crt-static"
        if release and "windows" in selected
        else None
    )
    if feature is None:
        return env
    if "CARGO_ENCODED_RUSTFLAGS" in env:
        existing = env["CARGO_ENCODED_RUSTFLAGS"]
        env["CARGO_ENCODED_RUSTFLAGS"] = (
            existing + ("\x1f" if existing else "") + f"-C\x1ftarget-feature={feature}"
        )
    elif "RUSTFLAGS" in env:
        env["RUSTFLAGS"] += f" -C target-feature={feature}"
    else:
        build_flags = []
        targets = {}
        for config in reversed(configs):
            value = config.get("build", {}).get("rustflags", [])
            if isinstance(value, str):
                build_flags = _flags(value)
            else:
                build_flags.extend(value)
            for name, values in config.get("target", {}).items():
                value = values.get("rustflags", [])
                if isinstance(value, str):
                    targets[name] = _flags(value)
                else:
                    targets.setdefault(name, []).extend(value)
        build_flags.extend(_flags(env.get("CARGO_BUILD_RUSTFLAGS", "")))
        target_flags = targets.get(selected, []) + _flags(
            env.get(f"CARGO_TARGET_{selected.upper().replace('-', '_')}_RUSTFLAGS", "")
        )
        predicates = [
            name for name, flags in targets.items() if name.startswith("cfg(") and flags
        ]
        if predicates:
            cfg = set(
                subprocess.check_output(
                    ["rustc", "--print", "cfg", "--target", selected], text=True
                ).splitlines()
            )
            for name in sorted(predicates):
                if _matches_cfg(name, cfg):
                    target_flags.extend(targets[name])
        # Target flags override build.rustflags; encoded flags preserve arguments
        # containing spaces and ensure Cargo's capability probe sees the CRT flag.
        env["CARGO_ENCODED_RUSTFLAGS"] = "\x1f".join(
            [*(target_flags or build_flags), "-C", f"target-feature={feature}"]
        )
    return env
