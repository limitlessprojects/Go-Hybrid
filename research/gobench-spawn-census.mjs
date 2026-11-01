import fs from 'node:fs';

const API = 'https://api.github.com/repos/goodmorning-coder/gobench';
const h = { 'user-agent': 'research', accept: 'application/vnd.github+json' };

async function goFiles() {
  const t = await (await fetch(`${API}/git/trees/master?recursive=1`, { headers: h })).json();
  return (t.tree || []).filter(e => e.type === 'blob' && /^gobench\/goker\/nonblocking\/[^/]+\/[^/]+\/.*\.go$/.test(e.path)).map(e => e.path);
}

function classify(line) {
  const m = line.match(/^\s*go\s+(.+)$/);
  if (!m) return null;
  const rest = m[1].trim();
  if (/^func\s*\(/.test(rest)) return 'closure';
  if (/^[A-Za-z_][\w.]*\(/.test(rest)) return /^[A-Za-z_]\w*\(/.test(rest) ? 'fn' : 'method';
  if (/^<-/.test(rest)) return 'recv';
  return 'other';
}

const files = await goFiles();
const rows = [];
for (const path of files) {
  const [owner, proj, id] = path.split('/').slice(3, 6);
  const raw = `https://raw.githubusercontent.com/goodmorning-coder/gobench/master/${path}`;
  const src = await (await fetch(raw, { headers: h })).text();
  const lines = src.split(/\r?\n/);
  const spawns = [];
  lines.forEach((l, i) => {
    const k = classify(l);
    if (k) spawns.push({ kind: k, line: i + 1, text: l.trim().slice(0, 90) });
  });
  const sends = lines.filter(l => /[^<]<-/.test(l) && !/^\s*\/\//.test(l)).length;
  rows.push({ kernel: `${proj}/${id}`, path, spawns, sendLines: sends });
  console.log(`${proj}/${id}: go=${spawns.length} ${JSON.stringify(spawns.map(s=>s.kind))} sends=${sends}`);
}

const all = rows.flatMap(r => r.spawns);
const by = k => all.filter(s => s.kind === k).length;
const summary = {
  kernels: rows.length,
  spawnSites: all.length,
  fnSpawns: by('fn'),
  methodSpawns: by('method'),
  closureSpawns: by('closure'),
  otherSpawns: by('other'),
  sendLines: rows.reduce((a, r) => a + r.sendLines, 0),
  methodPct: all.length ? +(100 * by('method') / all.length).toFixed(1) : 0,
  closurePct: all.length ? +(100 * by('closure') / all.length).toFixed(1) : 0,
  fnPct: all.length ? +(100 * by('fn') / all.length).toFixed(1) : 0
};
fs.writeFileSync('research/gobench-spawn-census.json', JSON.stringify({ collected: new Date().toISOString(), source: 'goodmorning-coder/gobench master', summary, rows }, null, 2));
console.log('\nSUMMARY', JSON.stringify(summary, null, 2));
