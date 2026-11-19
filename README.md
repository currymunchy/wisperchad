<p align="center"><img src="docs/logo-animated.gif" width="120" alt="wisperChad logo"></p>

<h1 align="center">wisperChad</h1>

<p align="center"><b>Talk your type.</b><br>
Free, private voice dictation and reply writing for Windows and Mac.<br>
Hold a hotkey, talk, and clean text, or a polished reply, appears wherever your cursor is.</p>

<p align="center">
  <a href="https://github.com/currymunchy/wisperchad/releases/latest/download/wisperChad-Setup.exe"><img alt="Download for Windows" src="https://img.shields.io/badge/Download_for_Windows-994ea8?style=for-the-badge&logo=windows&logoColor=white"></a>
  <a href="https://github.com/currymunchy/wisperchad/releases/latest/download/wisperChad.dmg"><img alt="Download for Mac" src="https://img.shields.io/badge/Download_for_Mac-1a1a1a?style=for-the-badge&logo=apple&logoColor=white"></a>
</p>

<p align="center">
  <a href="https://github.com/currymunchy/wisperchad/releases/latest"><img alt="Latest version" src="https://img.shields.io/github/v/release/currymunchy/wisperchad?label=version&color=994ea8"></a>
  <a href="LICENSE"><img alt="MIT License" src="https://img.shields.io/badge/license-MIT-994ea8"></a>
  <img alt="Windows 10 and 11, macOS 13.3 or newer" src="https://img.shields.io/badge/runs_on-Windows_%7C_macOS-994ea8">
  <img alt="Runs on your computer" src="https://img.shields.io/badge/cloud-none-994ea8">
</p>

<p align="center">
  <a href="https://currymunchy.github.io/wisperchad-app/">Website</a> ·
  <a href="#install">Install</a> ·
  <a href="#which-models-should-i-pick">Models</a> ·
  <a href="#privacy">Privacy</a> ·
  <a href="#build-from-source">Build from source</a> ·
  <a href="#support-wisperchad">Support</a>
</p>

<p align="center"><img src="docs/screenshot.png" width="860" alt="wisperChad's Home page, with a reply and a dictation it wrote"></p>

## What it does

| | |
|---|---|
| 🎙️ **Dictate anywhere** | Hold the hotkey and talk. Your words are typed at your cursor in any app: email, Word, Slack, WhatsApp, VS Code. The "um"s and "uh"s are taken out and punctuation is added for you. |
| ✉️ **Write your replies** | Open a message, click the reply box, hold the reply hotkey and say the gist, however rough. wisperChad reads the message and writes a polite reply in your voice. You read it and press Send yourself. |
| 🔒 **Stay private** | The speech and writing models run on your own computer. Your voice and your messages never leave it. No account, no subscription, no cloud. |

### See it in action

**Dictation: say it messy, get it clean**

> 🗣️ *"um so I was thinking we could uh meet on tuesday at like 3pm and go through the slides together"*
>
> ⌨️ I was thinking we could meet on Tuesday at 3 pm and go through the slides together.

**Replies: say the gist, get a polished email**

Sarah writes: *"Hi! We're planning a team lunch this Friday at 12:30. Can you make it? Also, do you have any
food allergies?"*

> 🗣️ *"yeah I can come, no allergies"*
>
> ⌨️ Hi Sarah,
>
> Yes, I can make it to the team lunch on Friday at 12:30. I don't have any food allergies.
>
> Best regards,<br>
> Alex

## Why

Tools like Wispr Flow are great, but they send your voice to the cloud and cost a monthly fee. Open-source
alternatives exist, but "write a reply for me" tends to sit behind a paid plan. wisperChad does the whole
thing on your computer, for free, under the MIT License.

## Install

### Windows

1. **[Download wisperChad-Setup.exe](https://github.com/currymunchy/wisperchad/releases/latest/download/wisperChad-Setup.exe)**
   and open it. It works on Windows 10 and 11.
2. If Windows SmartScreen warns about an unrecognised app, click **More info**, then **Run anyway**. The
   app isn't code-signed yet, because signing costs money.
3. Setup checks your computer, recommends models that fit it, downloads them and has you try a dictation
   and a reply.

### Mac

1. **[Download wisperChad.dmg](https://github.com/currymunchy/wisperchad/releases/latest/download/wisperChad.dmg)**.
   It's for Macs with an Apple chip (M1 or newer), on macOS 13.3 or newer.
2. Open it and drag wisperChad into **Applications**.
3. wisperChad isn't notarized yet, because that takes a $99-a-year Apple developer account, so macOS blocks
   it the first time. Try to open it, then go to *System Settings → Privacy & Security* and click
   **Open Anyway**. If macOS says the app is damaged, run this in Terminal and open it again:

   ```bash
   xattr -dr com.apple.quarantine /Applications/wisperChad.app
   ```

4. Setup asks for up to three permissions, each with an Allow button:
   - **Microphone**, to hear you.
   - **Accessibility**, to paste your text for you. Without it, your text stays on the clipboard for you
     to paste.
   - **Screen Recording**, for replies that read the open window. Reopen wisperChad after allowing it.

   Apps that aren't notarized can lose these after an update. If that happens, remove wisperChad from
   each list and add it again.

### Linux

There's no Linux installer yet, but wisperChad builds from source and its code has Linux support. Nobody
has tried it on a real Linux PC. See the Linux notes under [Platforms](#platforms), and [Build from source](#build-from-source).

## Hotkeys

| | Windows | Mac | Linux |
|---|---|---|---|
| Dictate | <kbd>Ctrl</kbd>+<kbd>Win</kbd> | <kbd>Ctrl</kbd>+<kbd>Option</kbd>+<kbd>D</kbd> | <kbd>Ctrl</kbd>+<kbd>Alt</kbd>+<kbd>Space</kbd> |
| Reply | <kbd>Ctrl</kbd>+<kbd>Win</kbd>+<kbd>Alt</kbd> | <kbd>Ctrl</kbd>+<kbd>Option</kbd>+<kbd>R</kbd> | <kbd>Ctrl</kbd>+<kbd>Alt</kbd>+<kbd>R</kbd> |

Change them in **Settings**: click the box and press the keys you want, like <kbd>Ctrl</kbd>+<kbd>Shift</kbd>+<kbd>D</kbd>
or just <kbd>F9</kbd>. Then pick how they work:

- **Hold to talk** (the default): hold the keys while you speak and let go to finish. Or double-press to
  keep listening without holding anything, and press once more to finish.
- **Press to start, press again to finish.**

On Windows, adding <kbd>Alt</kbd> turns a dictation that just started into a reply, so no words are lost.
Windows shortcuts like <kbd>Ctrl</kbd>+<kbd>Win</kbd>+<kbd>Left</kbd> still work, and a recording they set
off is thrown away. Hotkeys may not work while an app running as administrator is in front.

## Replies

By default, wisperChad reads the message from the open window. Open the email or chat, click into the
reply box and hold the reply hotkey. As you press it, wisperChad reads the window's text through the
system's accessibility features, including parts scrolled out of view, and takes a screenshot for any
pictures. On a Mac this works in Safari, Chrome, Brave, Edge, Mail and web-based apps like Slack.

- Your other tabs and windows aren't read, and neither are password fields or folded messages.
- Apps that don't share their text get the screenshot alone, so only what's on screen. Reading a
  screenshot needs a writing model that reads images, like Qwen3.5 4B.
- Prefer copying? Set *Settings → Reply reads the message from* to **Text you copied**, then select the
  message and press <kbd>Ctrl</kbd>+<kbd>C</kbd> before you hold the hotkey.

**The right form for the app.** In Gmail, Outlook and other mail apps a reply is an email, with a greeting
and a sign-off that fits. In WhatsApp, Teams, LinkedIn messaging and other chat apps it's just the
message. On a LinkedIn post, Reddit or YouTube it's a short public comment.

<details>
<summary><b>How wisperChad keeps your reply right</b></summary>

<br>

A reply takes two steps. First wisperChad finds the message you're replying to: the one just above the
box you clicked into, copied from the page's text, with the screenshot used for any pictures, charts or
tables in it. Then it turns what you said into your reply to that message.

Say it however it comes out, casual, unsure or angry, and it writes what a skilled professional in your
place would send to get what you want, keeping your stance. How you feel tells it what you want, not how
to sound: swear at a recruiter and it's a polite, firm no, swear at a rejection and it's gracious, swear
because something's great and it's warm. Swearing never reaches the reply. A question you say is asked of
them, not answered, and their own lines are never handed back to them. Say "tell him I can do noon" and
it tells him you can do noon. If you wrote last and they haven't answered, it writes your follow-up; put
your name in Settings so it can tell your messages from theirs.

What the model reads is checked against the page: a message it didn't copy from the page gives way to
the page's own text, and a message only counts as yours when the page shows your name, "me" or "You" as
its sender. What it writes is checked against what you said: your own news told back to you as theirs
("sorry to hear your laptop died"), the time you offered turned down, a no where you said yes or a yes
where you said no, a link of theirs sent back to them, or a time, day or reason of yours dropped or made
up. Any of these and it's asked again, pointedly, up to twice. Small models like Qwen3.5 2B make these
slips; bigger ones rarely do.

The message is read while you're still talking, from the moment the hotkey goes down, so once you let go
only the writing is left. Sign-offs fit the message: "Thank you," when you ask them for something,
"Kind regards," when you say no or sorry or just thank them, "Cheers," to someone who writes casually,
and "Best regards," otherwise.

</details>

**Dictate in one language, get it typed in another.** Pick the language under *Settings → Write dictation
in*. This needs a writing model, and translations are drafts too, so check them. Listening is in English
for now.

## Which models should I pick?

You don't have to. Setup recommends one of three presets for your computer:

| Preset | Recommended for | Writes with |
|---|---|---|
| Transcription only | Under 8 GB of RAM | Nothing: it types what you say, with no clean-up and no replies |
| Light | Basic computers with no graphics card | Qwen3.5 2B |
| Balanced | A graphics card with 6 GB of memory, and 6 GB of RAM | Qwen3.5 4B |

All three listen with Parakeet v2. To choose for yourself later, the **Models** page keeps it simple:

| | Model | Pick it if |
|---|---|---|
| Speech | Parakeet TDT 0.6B v2 | You speak English. Fast on any computer, no graphics card needed |
| Speech | Whisper Small (English) | You'd like an alternative to Parakeet |
| Writing | Qwen3.5 4B | You have 6 GB of RAM or more. The best results |
| Writing | Qwen3.5 2B | You have less memory. It can misread pictures, so check the draft |

<details>
<summary><b>More models, memory use and Ollama</b></summary>

<br>

Worth a look on the **More models** tab, which shows the memory each model needs, its speed and how it
should run on your computer:

| | Model | Pick it if |
|---|---|---|
| Speech | Moonshine Base | You want the quickest and smallest. It stumbles if you talk very fast |
| Speech | Whisper Small Q5 | You want Whisper Small at under half the download |
| Writing | Gemma 4 E2B | You want quicker clean-up. It's slower at screenshots and can misread charts |
| Writing | Gemma 4 E4B | The same, with a larger model |

Only the Qwen3.5 and Gemma 4 models have been tested with the built-in engine, and the More models tab
says so on the others.

**Memory use:** the speech model stays loaded while the app runs (75 MB to 1.6 GB). The writing model
loads when you use it and unloads after the time set in *Settings → Keep writing model loaded for*
(5 minutes by default), so it doesn't hold your graphics memory all day.

**Without a graphics card**, or when something else has filled it, the built-in engine runs on the
processor. Qwen3.5 4B then takes a few seconds for clean-up and copied-text replies, and about a minute
for a reply that reads the open window.

**Already use [Ollama](https://ollama.com)?** If it's running when you go through setup, wisperChad asks
whether to use it and lists the models already in it, so nothing downloads twice. Switch any time in
*Settings → Writing engine*. You can also install any model from
[ollama.com/library](https://ollama.com/library) by typing its tag on the Models page, then open
**Requirements** and click **Measure on my PC** to see what it really uses. Avoid Qwen3 4B through
Ollama: it always reasons before it answers, so each dictation takes close to a minute. If a model writes
nonsense, check for an `OLLAMA_KV_CACHE_TYPE` environment variable, remove it and restart Ollama.

**Can't reach Hugging Face from your network?** Set the environment variable `WISPERCHAD_MODEL_MIRROR` to
any address that serves the same model files.

</details>

## Settings worth knowing

- **Microphone:** use one other than the system default.
- **Mute my speakers while recording** is on by default, so nothing playing ends up in your dictation.
- **Launch on startup:** with the window showing, or hidden in the system tray.
- **Auto submit dictation** is off by default. It presses Enter after your words are typed, which sends
  them straight away in most chat apps. Replies are never sent for you.
- **Background:** white, black or match your computer.

The **About** page shows your version, where your app data and log files are, and the open-source
licenses. The log only records errors, never what you say or what's on your screen.

## Privacy

- Audio never leaves your computer and is never written to disk.
- Your last 10 dictations and replies are saved on your computer so you can read them on Home. They
  never leave it, older ones are dropped, and each has a Delete button.
- Window text and screenshots are only read when you press the reply hotkey. They go only to the writing
  model on your computer and are never written to disk.
- Hotkeys made only of modifier keys, like <kbd>Ctrl</kbd>+<kbd>Win</kbd>, work by checking which keys
  are down, many times a second. wisperChad only looks for Ctrl, Alt, Shift and Win, and whether another
  key is down while one of those is held. It keeps nothing.
- The only network traffic is downloading models (from Hugging Face or Ollama) and the writing engine
  (from llama.cpp's GitHub releases, checked against a fixed hash before it runs), and talking to the
  writing engine on `localhost`.
- No accounts, no analytics, no ads.

## How it works

```
 hold hotkey ──► microphone ──► Parakeet or Whisper (speech → text) ──► writing model ──► paste at cursor
                                      on your computer                   on your computer, optional
                                                    ▲
                                Reply: + the message you're replying to
```

- **App:** [Tauri 2](https://tauri.app), with a Rust backend and a plain HTML, CSS and JavaScript UI.
- **Speech to text:** [whisper.cpp](https://github.com/ggml-org/whisper.cpp) through
  [whisper-rs](https://github.com/tazz4843/whisper-rs), or NVIDIA Parakeet and Moonshine through
  [sherpa-onnx](https://github.com/k2-fsa/sherpa-onnx). Models download from Hugging Face when you pick
  them.
- **Writing:** [llama.cpp](https://github.com/ggml-org/llama.cpp)'s server, run hidden in the background
  on your graphics card if there's room and otherwise on your processor, or
  [Ollama](https://ollama.com) if you already have it. Both are only reached on `localhost`.
- **Typing:** the result is put on the clipboard and a paste is pressed for you. It stays on the
  clipboard, so if you clicked away while it was being written, paste it yourself.
- Closing the window keeps wisperChad in the system tray, so the hotkeys keep working. Quit from the tray
  icon.

## Platforms

| | Windows | Mac (Apple chip) | Linux with X11 | Linux with Wayland |
|---|---|---|---|---|
| Installer | Yes | Yes | Build from source | Build from source |
| Tried on real hardware | Yes | Yes | No | No |
| Hotkey of modifiers alone | <kbd>Ctrl</kbd>+<kbd>Win</kbd> | <kbd>Ctrl</kbd>+<kbd>Cmd</kbd> | <kbd>Ctrl</kbd>+<kbd>Super</kbd> | No |
| Text pasted at your cursor | Yes | Yes, with Accessibility allowed | Yes | No, it's left on the clipboard |
| Writing engine | Vulkan graphics or processor | Metal graphics or processor | Vulkan graphics or processor | Vulkan graphics or processor |
| Reply reads the open window's text | Yes | Web pages and Mail | No, the screenshot alone | No, the screenshot alone |
| Mute speakers while recording | Yes | Yes, except speakers with no mute switch, like HDMI screens | No | No |

<details>
<summary><b>Linux notes</b></summary>

<br>

- GNOME hides tray icons unless you add the AppIndicator extension. Without it, reopen the window by
  launching wisperChad again.
- **Wayland** is the default on most new Linux desktops. Run `echo $XDG_SESSION_TYPE` to see which one
  you're on. Wayland keeps apps from watching keys or typing into each other, so:
  - Hotkeys only work while an older (X11) app is in front. For a hotkey that works everywhere, add a
    shortcut in your desktop's keyboard settings that runs `wisperchad --dictate`, and another for
    `wisperchad --reply`. Each press starts listening, and the next one finishes.
  - Your text lands on the clipboard instead of at your cursor, and the listening bar reminds you to
    press <kbd>Ctrl</kbd>+<kbd>V</kbd>.
  - Set *Reply reads the message from* to **Text you copied**, because screenshots may fail.
- Choosing an X11 session on your login screen avoids all of this.

</details>

## Build from source

You'll need:

- [Rust](https://rustup.rs) (stable) and [Node.js](https://nodejs.org) 18 or newer
- [LLVM/Clang](https://releases.llvm.org) and [CMake](https://cmake.org), because whisper.cpp is compiled
  from source
- **Windows:** Visual Studio Build Tools with "Desktop development with C++"
- **Mac:** the Xcode Command Line Tools (`xcode-select --install`)
- **Linux**, on Ubuntu 24.04 or newer:

  ```bash
  sudo apt install build-essential clang cmake libclang-dev libwebkit2gtk-4.1-dev libgtk-3-dev libayatana-appindicator3-dev librsvg2-dev libasound2-dev libpipewire-0.3-dev libxcb1-dev libwayland-dev libegl-dev libgbm-dev libdrm-dev libdbus-1-dev
  ```

```bash
npm install
npm run dev                         # run it while you work on it
npm run build                       # build the installer, into src-tauri/target/release/bundle/
cd src-tauri && cargo test          # run the tests
```

On Windows, `npx tauri build --features vulkan` builds a version whose speech recognition runs on the
graphics card, with the [Vulkan SDK](https://vulkan.lunarg.com) installed.

**Tuning replies without a build.** The reply wording lives in `src-tauri/src/reply-prompts.json`. Copy
it to the app data folder (shown in About) and edit the copy: wisperChad reads it on every reply, and
About says when it's in use.

**Releases** are built by GitHub Actions. Pushing a tag like `v0.11.27` tests and builds the Windows
installer, the Microsoft Store package and the Mac app, and publishes them as the latest release.

<details>
<summary><b>Project layout</b></summary>

```
src-tauri/src/
  lib.rs          app setup, hotkeys, the record → transcribe → write → paste pipeline, UI commands
  chords.rs       hotkeys made only of modifier keys, like Ctrl+Win
  audio.rs        microphone capture and resampling to 16 kHz
  stt.rs          Whisper, Parakeet and Moonshine transcription
  engine.rs       the built-in writing engine (download, start, stop, chat)
  llm.rs          Ollama client (status, pull, delete, chat, memory measurement)
  prompts.rs      clean-up prompts, reply notes and output clean-up
  reply.rs        replies in two steps: find the message, then write the reply
  checks.rs       a reply's slips against what you said, each with what to ask the model to fix
  reply-prompts.json  the reply wording, which a copy in the app data folder can replace
  presets.rs      picks what to install for a computer
  presses.rs      tells a hold from a double-press
  speakers.rs     mutes the speakers while recording
  applog.rs       the error log
  catalog.rs      the model list with details and requirements (edit this to add models)
  hardware.rs     memory and graphics card detection for the requirements check
  input.rs        clipboard and paste
  screen.rs       screenshot of the active window for replies
  window_text.rs  text of the active window for replies
  settings.rs     settings stored as JSON
ui/
  index.html, app.js, styles.css   the main window
  splash.js                        the opening screen
  setup.js                         first-run setup and tests
  select.js                        dropdowns that stay inside the window
  about.js                         the About page
  theme.js                         black or white background, set before the page draws
  pill.html                        the floating "Listening…" bar
```

</details>

## Roadmap

- [ ] Get wisperChad on the Microsoft Store, signed by Microsoft
- [ ] Per-app tone: casual in Discord, professional in Outlook
- [ ] A custom dictionary for names and jargon
- [ ] Text that streams in while you talk
- [ ] Installers for Intel Macs and Linux
- [ ] Listening in more languages

## Support wisperChad

wisperChad is free and stays free. If it saves you time, you can
[buy me a coffee](https://buymeacoffee.com/phuasien) or [sponsor it on GitHub](https://github.com/sponsors/currymunchy).
It helps pay for code signing and testing on more computers.

<a href="https://buymeacoffee.com/phuasien"><img alt="Buy me a coffee" src="https://img.shields.io/badge/Buy_me_a_coffee-994ea8?style=for-the-badge&logo=buymeacoffee&logoColor=white"></a>
<a href="https://github.com/sponsors/currymunchy"><img alt="Sponsor on GitHub" src="https://img.shields.io/badge/Sponsor_on_GitHub-1a1a1a?style=for-the-badge&logo=githubsponsors&logoColor=white"></a>

## Contributing

Issues and pull requests are welcome, and so are reports from trying it on a Linux PC or an Intel Mac.
See [CONTRIBUTING.md](CONTRIBUTING.md).

## License

Made by Phua Si En. Released under the [MIT License](LICENSE). The full list of open-source licenses, and
the license of each AI model, is in the app under *About → Open-source licenses*. Models are downloaded
from their publishers and keep their own licenses.

wisperChad is an independent project and isn't affiliated with Wispr, OpenAI, Ollama or any model
provider.
