# Manual test checklist

Run through this before a release. Automated unit tests: `cd src-tauri && cargo test`.

## First run
- [ ] Opening the app plays the opening screen for about 1.25 seconds, on white in both themes, then shows the app
- [ ] Close the window, then open it again from the tray menu, the Start menu or the desktop shortcut: the opening screen plays again
- [ ] Starting hidden at login shows no opening screen until you open the window
- [ ] Reopening the window from the tray doesn't play it, and neither does starting hidden with Windows
- [ ] Fresh install opens the setup instead of the app
- [ ] The presets page shows this computer's memory, graphics card and processor correctly
- [ ] AMD and Intel graphics cards with 2 GB or more show up, and ones built into the processor don't
- [ ] This PC's recommended preset is selected, and Install shows its download size
- [ ] Clicking another preset changes the download size, and a heavy one warns it may run slowly
- [ ] Customize starts from the chosen preset, and picking other models changes what installs
- [ ] A custom writing model that can't read images sets Reply to copied text
- [ ] With no Ollama, setup says there's nothing else to install, and models install without visiting Settings
- [ ] With Ollama running, setup stays on the built-in engine and asks if you'd rather use Ollama, saying how many models are in it
- [ ] Clicking Use Ollama opens Customize, with the models already in Ollama listed
- [ ] Transcription only: setup says replies won't work, and still offers Ollama if it's running
- [ ] With no writing model, the Reply hotkey is empty on Home and in Settings, and comes back once one is added
- [ ] Ollama running at first launch but closed before setup finishes: setup moves to the built-in engine
- [ ] Every preset listens with Parakeet v2
- [ ] Under 8 GB of RAM, Transcription only is recommended, and it says there are no replies
- [ ] Transcription only skips the reply test and the name, and installs no writing model or engine
- [ ] After Transcription only, the reply hotkey says replies need a writing model
- [ ] The Customize tab can pick None for the writing model
- [ ] Setup offers three presets: Transcription only, Light and Balanced; Qwen3.5 9B is only on More models
- [ ] A PC with no graphics card gets Light, however fast its processor, and Balanced warns it may run slowly; a 16 GB PC with an 8 GB card gets Balanced
- [ ] Installing shows both models' progress, and a failed download offers Try again and Skip setup for now
- [ ] Dictate test: holding the hotkey and speaking fills the box and says it works
- [ ] Dictate test: the dictation hotkey box shows Ctrl+Win; clicking it and holding other keys changes it, and the steps below show the new keys
- [ ] Dictate test: after the first dictation, a pop-up says you can also double-tap, and Close hides it; with "Press to start, press again to finish" there's no pop-up
- [ ] Next works on both tests without trying them, and the last page then says a test was skipped
- [ ] Reply test works with either a hold or a double-tap, and its steps say both
- [ ] Reply test: the reply hotkey changes the same way, and can't be the dictation one
- [ ] With the microphone allowed for desktop apps, setup goes from your name straight to the dictate test
- [ ] With Settings → Privacy → Microphone off for desktop apps, setup shows Allow wisperChad: Allow opens that setting, and switching it on turns the row to Allowed by itself
- [ ] Right after installing, setup asks for your name: Next or Enter saves it to Settings, Skip leaves it as it was, and the test reply to Sarah is signed with it
- [ ] The listening bar never takes focus: the title bar stays active while it shows
- [ ] With words in the box, one click on Next moves on, in both the dictate and reply tests
- [ ] Dictate test: saying nothing asks you to check the microphone
- [ ] Reply test: the draft lands in the box and answers Sarah's email without any copying
- [ ] Closing the app partway reopens setup next time, without downloading finished models again
- [ ] Finishing, or Skip setup for now, means it doesn't come back on the next launch
- [ ] Upgrading from an older version with models installed doesn't show it
- [ ] Settings → Run setup again opens all of setup, with Cancel on it
- [ ] Models → Choose a preset for me opens only the presets: picking one installs what it needs and switches to it, with no name, permissions or tests, and says what it's now using
- [ ] The Models page says which preset is in use beside Choose a preset for me ("Current preset: Light"), or Custom for models picked by hand
- [ ] Choose a preset for me starts on the preset in use, marked In use, with "Currently applied" greyed out; picking another one turns it back into Use this preset
- [ ] With models picked by hand, it starts on the recommended preset
- [ ] Cancel leaves your models as they were
- [ ] A preset that's already installed says so and goes straight to the tests

## Models
- [ ] On a PC without AVX-512 (like a Ryzen 7 3700X), every Whisper model dictates without crashing, as do Parakeet and Moonshine
- [ ] With setup skipped, the app opens on the Models page, with two speech models and two writing models
- [ ] More models lists the rest, each showing memory, disk, speed and how it should run on this PC
- [ ] A model installed from More models also shows on the Models page once it's in use
- [ ] Moonshine Base: a recording over 30 seconds comes out whole, with nothing repeated
- [ ] Gemma 4 E2B and E4B: clean-up and an open window reply both work, on a graphics card too
- [ ] Requirements panel shows RAM / VRAM / disk and a verdict for your PC
- [ ] Installing a speech model shows progress and auto-selects it
- [ ] Installing a Parakeet model downloads all its files with one progress bar, and dictation works with it
- [ ] Requirements on other models still open while something is downloading
- [ ] Installing a writing model shows progress and auto-selects it
- [ ] Use / Remove work for both kinds of model
- [ ] The writing model in use has Stop using: it turns clean-up and replies off, the Reply hotkey goes empty, and Use turns it back on

## Writing engine
- [ ] Fresh install on a PC without Ollama: Settings shows the built-in engine, no Ollama banner anywhere
- [ ] First writing model downloads the engine and the model on one progress bar, then "Getting the model ready"
- [ ] Second writing model doesn't download the engine again
- [ ] Interrupted download picks up with the files it still needs
- [ ] Dictation cleanup, a copied-text reply and an open window reply all work with no Ollama installed
- [ ] Task Manager: `llama-server.exe` appears on first use and goes after "Keep writing model loaded for"
- [ ] Quitting wisperChad, or ending it in Task Manager, ends `llama-server.exe` too
- [ ] Fill the GPU with something else, then dictate: it still works, more slowly, on the CPU
- [ ] Too little memory for the model: the pill says so and dictation still pastes the raw transcript
- [ ] Removing a model that's loaded unloads it first

## With Ollama
- [ ] Fresh install, choosing Ollama in setup: Settings shows Ollama, and its models appear in the list
- [ ] Upgrading from a version before 0.6.0 stays on Ollama with the same model selected
- [ ] Models page offers to switch between engines, and Settings → Writing engine does too
- [ ] Switching to Ollama unloads the built-in engine's model
- [ ] Measure on my PC shows real memory use, or the error if the model can't load
- [ ] Custom Ollama tag: a bad tag shows an error, a good one appears in the list
- [ ] Ollama closed: banner on Models page offers the built-in engine, Home checklist updates

## Dictation
- [ ] The listening bar has a × while it listens, transcribes or writes: clicking it hides the bar, types nothing and unmutes your music, and the hotkey works again straight away
- [ ] A × on an error closes it
- [ ] Hold hotkey, speak, release: text appears at the cursor, filler words removed
- [ ] Dictating a request ("write me an email to my boss asking for Friday off", "make me a document that says...", "what time is it in London") types the request, tidied: never an answer, an email or a refusal
- [ ] Dictating something a model might refuse ("How to sell drugs.", "How do I make a bomb?") types exactly that, tidied, never "I cannot provide..."
- [ ] Silence or a short tap does nothing
- [ ] With Ollama closed, raw transcript is still typed
- [ ] With no speech model: pill says to pick one

## Reply
- [ ] Copy a message, click the reply box, hold the reply hotkey, speak: a polite draft appears
- [ ] Afterwards the clipboard holds the reply, so it can be pasted again by hand
- [ ] Pressing the reply hotkey again without copying a new message says to copy it first, rather than replying to the last reply
- [ ] Name set in Settings: reply is signed with it
- [ ] Empty clipboard: pill says to copy the message first
- [ ] No writing model: pill says reply needs one
- [ ] Open window mode: open an email, click the reply box, hold the reply hotkey, speak: the draft answers the open email
- [ ] On a PC with a graphics card, a reply to an email lands within a couple of seconds of letting go of the hotkey: the message is read while you talk
- [ ] With Light on a graphics card, a reply to a long Gmail email (with a legal notice under it) lands in under 5 seconds from pressing the hotkey, saying just "okay thanks"
- [ ] Cancelling a reply while you talk, or saying nothing, types nothing, and the next reply is as quick as ever
- [ ] An email reply to a person greets them by first name ("Hi Farahnadhia,"), and one to a team ("Early Careers AP") just "Hi,"
- [ ] With Light (Qwen3.5 2B), an email from a person signed off by a team (DTSA's) still greets the person by name, never "Hi DTSA Team," or you
- [ ] The same in Gmail in Brave or Chrome, five times in a row: "Hi Farahnadhia," every time
- [ ] A reply never borrows the sender's own words about themselves, like "to help us facilitate the session"
- [ ] An angry or sweary note ("tell this bastard I'm not interested") gives a calm, firm reply that keeps what you meant, never a refusal or a lecture
- [ ] An angry no to an invitation ("Boy, f*** you, I ain't going") gives a polite no, never a yes or a thank-you for having you
- [ ] A reply never sends them their own link (like DTSA's "Submit your idea here" form) unless you said to
- [ ] A plain "thank you" to a bank's newsletter just thanks them: no "contact us" or "we are here to help" in their voice, no promises, no follow-up, even with only the footer on screen
- [ ] Writing for your own shop ("tell her we're open till 6, she can contact us"), the reply keeps "contact us"
- [ ] "I don't care", "whatever" or "meh" to an email asking you to share or buy something gives a polite no thanks, never "I don't care" and never a yes
- [ ] "I don't care" to "Tuesday or Thursday?" says either day works for you
- [ ] Following up your own unanswered email (with your portfolio link in it) greets who you wrote to, never you, and leaves the link out
- [ ] A note that's only swearing gives what a sensible professional would send: gracious to a rejection, a polite no to a pitch, a short "thanks for the update" to a notice
- [ ] "where the fuck is my parcel" asks them to check where it is, in your voice, not theirs
- [ ] A reply never hands their own lines back, like wishing a company well "in your studies"
- [ ] With your name in Settings, a LinkedIn chat where you wrote last and got no answer gets a short, polite follow-up to them, never a reply to yourself
- [ ] A chat opened over the LinkedIn feed or a profile gets a chat reply, not a post comment
- [ ] A message that opens "Hi <your name>" is treated as theirs, never yours
- [ ] Happy swearing ("this is fucking amazing") on a post gives a warm, enthusiastic comment, not a firm one
- [ ] "damn ok, can I apply next year" on a rejection is gracious and asks the question
- [ ] Kind words about them ("he's so nice, tell him yes") stay kind
- [ ] No reply ever contains a swear word or "HOW I FEEL"
- [ ] A short note like "ok thanks bro" gives a full, polite reply that fits the message, like thanking them for the update
- [ ] Replies keep who's who: "my" stays yours, and their message is never repeated back to them
- [ ] A question you say ("can I still apply next year?") is asked of them, never answered for them
- [ ] A reason that would put them off ("I'm too lazy to go") is left out for a polite one, like having something else on; a real one ("I'm sick") stays
- [ ] "great post I agree" agrees and "this take is wrong" disagrees, whatever the post says
- [ ] "Tell him I can do noon" to "redo it by 8am" says you can't do 8am but can do noon, to him, not his side of it
- [ ] With Light (Qwen3.5 2B): a WhatsApp chat with a list of chats above it, where they wrote last, gets a reply to their message, never a follow-up to yourself
- [ ] With Light: a recruiter's LinkedIn message gets your answer to it, never a follow-up as if you had written it
- [ ] With Light: a short note full of slang ("nah bro im broke too") comes back as a proper reply, not as you said it
- [ ] With Light: "Sorry, my laptop died, can I still get full marks?" to a lecturer says your laptop died, never "sorry to hear about your laptop"
- [ ] With Light: "9 is too early, tell her I can do 2pm" offers 2pm; "Can't make it, maybe next week" keeps "next week"; "I'll send it tonight" never turns into a no
- [ ] With Light: "totally agree" and "nah, that's wrong" on a post keep their side, whatever the post says
- [ ] LinkedIn post: a short comment with no greeting or sign-off, on the post just above the comment box, not the next one
- [ ] LinkedIn messaging: a message to that person, starting with their name if natural, no sign-off
- [ ] An email with a chart or picture: the reply uses what the picture shows
- [ ] A reply-prompts.json in the app data folder changes the wording on the next reply without restarting, and About says it's in use; broken JSON is ignored and logged
- [ ] No "[Your Name]" placeholders, and your email address never ends up as the signature
- [ ] If the model still refuses, nothing is pasted and the listening bar says to try saying it another way
- [ ] Open window mode, long email scrolled partway: mention a detail from out of view: the draft uses it
- [ ] Open window mode, email with a picture on screen: the draft uses what's in the picture, on a PC with a graphics card
- [ ] Without a graphics card, a reply to an open window skips reading pictures and is much quicker
- [ ] Open window mode with other tabs open: the draft doesn't mention them
- [ ] Open window mode in an app that shares no text: still drafts from the screenshot
- [ ] Open window mode with a model that can't read images: pill says so, nothing is pasted
- [ ] Open window mode doesn't touch the clipboard check: no "copy first" message

- [ ] A Chinese draft doesn't start with a 主题： subject line

## Replies
- [ ] A comment on one post in a LinkedIn feed is about that post, not the ones around it
- [ ] Saying "you are awesome" on a post gives a compliment to them, not "Thank you for your kind words"
- [ ] A reply never speaks as the company or person being replied to
- [ ] What's pasted matches Recent every time, even when the PC is busy, and never your old clipboard
- [ ] What was typed stays on the clipboard: click out of the box while it's being written, then press Ctrl+V and it's all there
- [ ] An email gets a greeting, a sign-off and your name
- [ ] Your name sits right under "Best regards," with no blank line between them, on Light too
- [ ] Sign-offs fit the email: "Thank you," when you ask for something, "Kind regards," when you say no or sorry or only thank them (as for a rejection), "Cheers," to a casual "Cheers, Tom" email, "Best regards," otherwise
- [ ] Reply tone casual: emails sign off "Cheers,"; friendly: "Warm regards,"
- [ ] A LinkedIn or WhatsApp reply has no sign-off and no name, even in a professional tone
- [ ] Copied-text replies follow the same rules as window replies, in Gmail and in WhatsApp
- [ ] A subject mentioning WhatsApp in Gmail still gets a sign-off
- [ ] A desktop app whose title is only the chat's name (Teams, WeChat) still gets a chat reply, and a desktop Outlook email whose title is only the subject still gets an email
- [ ] With no name in Settings, an email is signed off with no name
- [ ] Every reply comes out in English, even to a message in another language

## Dropdowns
- [ ] Every dropdown in Settings opens inside the window, and opens upward near the bottom edge
- [ ] Arrow keys and Escape work, and clicking away or scrolling closes it

## Recent chats
- [ ] Home shows the last 10 dictations and replies, newest first, and an 11th pushes out the oldest
- [ ] They're still there after quitting and reopening wisperChad
- [ ] Delete removes that chat only, and it stays gone after reopening

## Hotkeys
- [ ] Hold, speak, release: finishes on release, with no delay
- [ ] Double-press, speak with nothing held, press once more: the words from the first press are included
- [ ] Double-press works with a normal hotkey, with Ctrl+Win, and with Ctrl+Win+Alt
- [ ] A single quick tap does nothing and leaves no recording running
- [ ] While one hotkey keeps listening, the other hotkey does nothing
- [ ] Change both hotkeys in Settings; old ones stop working, new ones work immediately
- [ ] Re-capturing the current hotkey doesn't start a recording
- [ ] Single F-key hotkeys work
- [ ] Set Dictate to Ctrl+Win and Reply to Ctrl+Win+Alt by holding the keys in each box and letting go
- [ ] Hold Ctrl+Win, speak, release in either order: text appears, Start doesn't open
- [ ] Ctrl+Win also works with wisperChad's own window in front, in setup
- [ ] Hold Ctrl+Win, add Alt, speak, release: a reply appears with the first words in it, no menu bar opens
- [ ] Ctrl+Win+Left still switches desktops and leaves no recording running
- [ ] Win alone still opens Start, and Ctrl+C still copies
- [ ] Pressing Ctrl or Win alone in a hotkey box saves nothing
- [ ] "Press to start, press again" style works with Ctrl+Win and Ctrl+Win+Alt
- [ ] "Press to start, press again" style works
- [ ] Reset to default works

## Recording
- [ ] Every microphone is listed, and picking one records from it
- [ ] With the chosen microphone unplugged, it falls back to the system default
- [ ] With Mute on, music stops while recording and comes back after, including when a recording is cancelled
- [ ] Speakers you muted yourself stay muted afterwards
- [ ] Auto submit presses Enter after a dictation, in a real chat box, with the words already in it
- [ ] Auto submit never presses Enter after a reply

## Startup
- [ ] Launch and show the window: after signing in, the window opens
- [ ] Launch hidden: after signing in, only the tray icon shows and hotkeys work
- [ ] Opening the app yourself always shows the window, even on Launch hidden
- [ ] Don't launch: nothing starts at sign-in

## Microsoft Store
- [ ] The Store package installs from the .msix, opens, and dictates
- [ ] Installing a writing model downloads only the model, never the engine
- [ ] Settings shows the Windows Settings note in place of Launch on startup
- [ ] About shows, and its Open buttons open, the real folders under Packages\PhuaSiEn.wisperChad_...\LocalCache
- [ ] Turned on in Windows Settings → Apps → Startup, wisperChad starts when you sign in

## About
- [ ] Shows the right version, and "Made by Phua Si En"
- [ ] Donate, and Support wisperChad at the bottom of the sidebar, open the website's Support section in the browser, with Buy me a coffee and Sponsor on GitHub
- [ ] See all licenses shows the full list, and Back to About returns
- [ ] More models no longer lists Gemma 3 or Llama 3.2
- [ ] After an update, a small note shows what's new once, and closes with the ×
- [ ] A fresh install doesn't show the note
- [ ] Both Open buttons open the right folder
- [ ] A failed reply adds a line to the log, with none of your words in it

## Name
- [ ] The Start menu, taskbar, tray tooltip, window title and uninstall list all say wisperChad
- [ ] Installing over an older version keeps settings, models and recent chats, with one copy installed

## Appearance
- [ ] A fresh install starts on White, with Ctrl+Win to dictate and Ctrl+Win+Alt to reply
- [ ] Reply reads the open window by default, with nothing to copy first
- [ ] A hotkey another app owns says so by name, and doesn't stop setup
- [ ] The Start menu, taskbar, tray, installer and sidebar show the black logo
- [ ] Settings → Background switches to Black or White straight away, on every page and in setup
- [ ] The choice survives closing and reopening, with no flash of the other background
- [ ] The listening bar is white with black words on White, and black with white words on Black
- [ ] Changing Background is picked up by the listening bar on the next hotkey press, with no restart
- [ ] Menus, scrollbars and dropdowns match the background
- [ ] Nothing is squashed at the smallest window size

## App
- [ ] Closing the window keeps hotkeys working (tray)
- [ ] Launching a second time brings the window back instead of a second copy

## macOS
None of this has been run on a real Mac yet. Everything above applies too, with Cmd for Win and Option for Alt.
- [ ] Setup shows Allow wisperChad with Microphone, Accessibility and Screen Recording, and each Allow asks or opens its switch
- [ ] With all three on, the page moves on by itself, and the dictate test hears you
- [ ] The downloaded app opens after Open Anyway in Privacy & Security
- [ ] The first dictation asks for the microphone and goes on to work
- [ ] Without Accessibility, the pill says so and the text is on the clipboard
- [ ] With Accessibility, text pastes at the cursor in another app, on a non-QWERTY layout too
- [ ] Open window reply without Screen Recording: the pill says so. With it: the draft uses what's on screen
- [ ] Gmail in Safari, then in Chrome or Brave, with the window made small: a reply to a long email
  uses a detail scrolled out of view, and a plain "thank you" just thanks them
- [ ] An email in Mail: the reply answers it, not the drafts or the message list
- [ ] Without Accessibility on, an open window reply still works, from the screenshot
- [ ] Telegram Web or WhatsApp Web in a browser: the reply answers the last message above the box
- [ ] On a fresh install, Ctrl+Option+D dictates and Ctrl+Option+R replies
- [ ] The dictate test in setup fills its box, with wisperChad in front
- [ ] Music stops while recording and comes back after, and speakers muted by hand stay muted
- [ ] Cmd+Q while recording: the sound comes back
- [ ] Launch on startup opens wisperChad after logging in, and the hidden choice leaves the window closed
- [ ] Ctrl+Cmd works as a hotkey, with no permission asked for it
- [ ] Ctrl+Cmd+Left, or any shortcut starting from Ctrl+Cmd, leaves no recording running
- [ ] On any Apple Silicon Mac, an M1 too, every Whisper model dictates without crashing
- [ ] The first writing model downloads the engine, and Activity Monitor shows `llama-server`
- [ ] Quit, or Force Quit: `llama-server` goes within a few seconds
- [ ] The pill doesn't take focus from the app being typed in, and shows over a full-screen app
- [ ] Closing the window keeps the app in the menu bar, and clicking the Dock icon brings it back
- [ ] Settings says Cmd and Option, and the reply steps say Cmd+C
- [ ] Both .dmg files open: Apple Silicon, and Intel if you have one

## Linux with X11
None of this has been run on real hardware yet. Everything above applies too, with Super for Win.
- [ ] The .deb installs on Ubuntu 24.04 and on the newest Ubuntu, and the AppImage runs on both
- [ ] Hold a hotkey, speak, release: text appears at the cursor
- [ ] Ctrl+Super works as a hotkey, and Ctrl+Super+Left leaves no recording running
- [ ] An open window reply uses what's on screen
- [ ] The first writing model downloads the engine, and `ps` shows `llama-server`
- [ ] Quit, or `kill -9` wisperchad: `llama-server` goes within a few seconds
- [ ] Without `libgomp1` (AppImage): the pill names the missing library
- [ ] An NVIDIA or AMD GPU shows on the Models page, and the engine uses it through Vulkan

## Linux with Wayland
- [ ] Home shows the Wayland banner
- [ ] Setting Ctrl+Super as a hotkey says it needs X11, and the old hotkey stays
- [ ] A desktop shortcut running `wisperchad --dictate` starts listening, and the next press finishes
- [ ] `wisperchad --reply` does the same for a reply
- [ ] `wisperchad --dictate` with the app closed opens it, and the shortcut works from then on
- [ ] After speaking, the pill says to press Ctrl+V, and that pastes the text in a Wayland app and an X11 app
