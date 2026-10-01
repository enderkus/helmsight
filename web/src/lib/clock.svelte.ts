// A shared clock so relative times ("12s ago") update together.

export const clock = $state({ now: Math.floor(Date.now() / 1000) });

let started = false;

export function startClock(): void {
  if (started) return;
  started = true;
  setInterval(() => {
    clock.now = Math.floor(Date.now() / 1000);
  }, 1000);
}
