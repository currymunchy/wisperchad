// The opening screen: the logo's waves move in purple, settle back to white, then it fades into the
// app, which gets ready behind it. It plays when the app starts and every time the window is opened
// again from the tray or its shortcut. Time only counts once the window is painting, so a window that
// starts hidden never shows it, and the timer clears it from there.
(function () {
  const splash = document.querySelector("#splash");
  const mark = splash.querySelector(".splash-mark");
  const bars = [...splash.querySelectorAll(".splash-waves rect")];

  const HEIGHTS = [80, 176, 250, 176, 80];
  const SWING = [0.8, 0.5, 0.38, 0.5, 0.8];
  const TALLEST = [150, 270, 340, 270, 150];
  const SHORTEST = 44;
  const MIDDLE = 586;
  const WHITE = [255, 255, 255];
  const PURPLE = [153, 78, 168];
  const FADE_AT = 1;
  const FADE_FOR = 0.25;
  const GIVE_UP_AFTER_MS = 3000;

  const smooth = (x) => {
    const t = Math.min(1, Math.max(0, x));
    return t * t * (3 - 2 * t);
  };
  const moving = (t) => smooth((t - 0.07) / 0.17) * (1 - smooth((t - 0.65) / 0.25));

  function drawBar(bar, i, t) {
    const away = Math.abs(i - 2); // The sides mirror each other, spreading out from the middle.
    const amount = moving(t - away * 0.02);
    const turn = 2 * Math.PI;
    const wave = 0.55 * Math.sin(turn * (3.6 * t - 0.22 * away))
      + 0.3 * Math.sin(turn * (5.8 * t - 0.37 * away) + 1.1)
      + 0.15 * Math.sin(turn * (9 * t - 0.5 * away) + 2.3);
    const height = Math.min(TALLEST[i], Math.max(SHORTEST, HEIGHTS[i] * (1 + amount * SWING[i] * wave)));
    bar.setAttribute("height", height.toFixed(1));
    bar.setAttribute("y", (MIDDLE - height / 2).toFixed(1));
    const colour = WHITE.map((c, k) => Math.round(c + (PURPLE[k] - c) * amount));
    bar.setAttribute("fill", `rgb(${colour.join(",")})`);
  }

  function draw(t) {
    bars.forEach((bar, i) => drawBar(bar, i, t));
    const entered = smooth(t / 0.15);
    mark.style.opacity = entered;
    mark.style.transform = `scale(${0.96 + 0.04 * entered})`;
    splash.style.opacity = 1 - smooth((t - FADE_AT) / FADE_FOR);
  }

  // Each play has its own number, so a play that's been replaced stops drawing.
  let plays = 0;
  let giveUp;
  function play() {
    const run = ++plays;
    const done = () => run === plays && splash.classList.add("hidden");
    let start = null;
    function tick(now) {
      if (run !== plays) return;
      start ??= now;
      const t = (now - start) / 1000;
      if (t >= FADE_AT + FADE_FOR) return done();
      draw(t);
      requestAnimationFrame(tick);
    }
    draw(0);
    splash.classList.remove("hidden");
    requestAnimationFrame(tick);
    clearTimeout(giveUp);
    giveUp = setTimeout(done, GIVE_UP_AFTER_MS);
  }

  play();
  window.__TAURI__?.event?.listen("app-opened", play);
})();
