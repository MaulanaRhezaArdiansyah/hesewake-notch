// Plays Claude Code hook events through the extension's real hook command, then stays alive like a running agent.
const { spawnSync } = require('child_process');
const fs = require('fs');
const [exe, file] = process.argv.slice(2);
const events = JSON.parse(fs.readFileSync(file, 'utf8'));
const sleep = (ms) => new Promise((r) => setTimeout(r, ms));
(async () => {
  for (const e of events) {
    const wait = e.wait || 300;
    delete e.wait;
    const r = spawnSync(exe, ['--hook', 'claude'], { input: JSON.stringify(e) });
    console.log(new Date().toISOString(), e.hook_event_name, e.session_id, 'exit', r.status);
    await sleep(wait);
  }
  await sleep(600000);
})();
