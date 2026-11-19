"""Writes ui/licenses.js: the credits and license texts for everything the app is built on.
Run it from anywhere before a release, after dependencies change: python scripts/licenses.py
For the Mac app, name its platform: python scripts/licenses.py aarch64-apple-darwin

The Rust libraries come from `cargo metadata`, and their license files from cargo's download folder.
Libraries that share a license text are listed together, with their own copyright lines, so the
text appears once."""
import json, os, re, subprocess, sys

ROOT = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
OUT = os.path.join(ROOT, "ui", "licenses.js")
PLATFORM = sys.argv[1] if len(sys.argv) > 1 else "x86_64-pc-windows-msvc"
LICENSE_FILE = re.compile(r"^(licen[cs]e|copying|notice|unlicense)", re.I)
COPYRIGHT = re.compile(r"^\s*(copyright\b|\(c\)|©)", re.I)

HEADER = """wisperChad is released under the MIT License. It is built on the open-source work and AI models
below. Thank you to everyone who made them.

AI MODELS
The app downloads these from their publishers only when you choose them.

Whisper (English and multilingual, Large v3 Turbo)
  By OpenAI. MIT License. Converted for whisper.cpp by the ggml authors.

Parakeet TDT 0.6B v2 and v3
  By NVIDIA. Creative Commons Attribution 4.0 International (CC BY 4.0),
  https://creativecommons.org/licenses/by/4.0/
  Changed from the original: converted to ONNX and quantised to 8 bits by the sherpa-onnx project.
  The model is provided as is, without warranties of any kind.

Moonshine Base (English)
  By Moonshine AI (Useful Sensors, Inc.). MIT License.
  Converted for sherpa-onnx by the sherpa-onnx project.

Qwen3 and Qwen3.5
  By the Qwen team, Alibaba Cloud. Apache License 2.0.

Gemma 4 E2B and E4B
  By Google. Apache License 2.0.

The GGUF files of Qwen3 1.7B, Qwen3.5 and Gemma 4 are by Unsloth, and of Qwen3 8B by the Qwen team.

ENGINES
whisper.cpp and llama.cpp: Copyright (c) 2023-2024 The ggml authors. MIT License.
  llama.cpp's llama-server is downloaded from its GitHub releases when you use the built-in
  writing engine.
sherpa-onnx: by the k2-fsa project (Next-gen Kaldi). Apache License 2.0.
ONNX Runtime: Copyright (c) Microsoft Corporation. MIT License.

RUST LIBRARIES
Each license text below is followed by the libraries it covers.
"""


def metadata():
    command = ["cargo", "metadata", "--format-version", "1", "--filter-platform", PLATFORM]
    for extra in (["--offline"], []):
        result = subprocess.run(command + extra, cwd=os.path.join(ROOT, "src-tauri"), capture_output=True, text=True,
                                encoding="utf-8")
        if result.returncode == 0:
            return json.loads(result.stdout)
    sys.exit(result.stderr)


def shipped_packages(meta):
    """Everything the app links: the normal dependencies of the app, and theirs, but not build tools."""
    nodes = {node["id"]: node for node in meta["resolve"]["nodes"]}
    packages = {package["id"]: package for package in meta["packages"]}
    root = meta["resolve"]["root"]
    seen, waiting = set(), [root]
    while waiting:
        current = waiting.pop()
        if current in seen:
            continue
        seen.add(current)
        for dependency in nodes[current]["deps"]:
            if any(kind.get("kind") is None for kind in dependency["dep_kinds"]):
                waiting.append(dependency["pkg"])
    seen.discard(root)
    return sorted((packages[i] for i in seen), key=lambda p: (p["name"].lower(), p["version"]))


def license_texts(package):
    folder = os.path.dirname(package["manifest_path"])
    names = sorted(name for name in os.listdir(folder) if LICENSE_FILE.match(name)
                   and os.path.isfile(os.path.join(folder, name)))
    texts = []
    for name in names:
        with open(os.path.join(folder, name), encoding="utf-8", errors="replace") as f:
            texts.append(f.read())
    return texts


def split(text):
    """The copyright lines, and the rest of the text with spacing evened out, which is what libraries share."""
    lines = text.replace("\r\n", "\n").split("\n")
    owners = [line.strip() for line in lines if COPYRIGHT.match(line)]
    body = "\n".join(line.rstrip() for line in lines if not COPYRIGHT.match(line)).strip()
    return owners, re.sub(r"\n{3,}", "\n\n", body)


meta = metadata()
groups, unfiled = {}, []
for package in shipped_packages(meta):
    label = f"{package['name']} {package['version']} ({package.get('license') or 'see text'})"
    texts = license_texts(package)
    if not texts:
        unfiled.append(label)
        continue
    for text in texts:
        owners, body = split(text)
        key = re.sub(r"\s+", " ", body).lower()
        group = groups.setdefault(key, {"body": body, "users": {}})
        group["users"].setdefault(label, []).extend(owners)

parts = [HEADER]
for group in sorted(groups.values(), key=lambda g: (-len(g["users"]), g["body"][:40])):
    parts.append("-" * 72)
    for label, owners in group["users"].items():
        parts.append(label + "".join(f"\n    {owner}" for owner in dict.fromkeys(owners)))
    parts.append("\n" + group["body"] + "\n")
if unfiled:
    parts.append("-" * 72)
    parts.append("These libraries ship no license file. Each is under the license named beside it, whose text is above.")
    parts.extend(unfiled)

text = "\n".join(parts) + "\n"
with open(OUT, "w", encoding="utf-8", newline="\n") as f:
    f.write("// Made by scripts/licenses.py. Don't edit by hand.\n")
    f.write("const THIRD_PARTY_LICENSES = " + json.dumps(text, ensure_ascii=False) + ";\n")
print(f"{len(groups)} license texts, {sum(len(g['users']) for g in groups.values())} uses, "
      f"{len(unfiled)} libraries without a file, {os.path.getsize(OUT) // 1024} KB")
