// Creates a funded throwaway testnet account for the video through the helper service.
// The key is written to a file in the scratchpad and never printed.
import { writeFileSync } from 'node:fs';
import { KeyPair } from 'near-api-js';
const kp = KeyPair.fromRandom('ed25519');
const stamp = new Date().toISOString().replace(/[-:TZ.]/g, '').slice(0, 14);
const id = `antumbra-demo-${stamp}.testnet`;
const res = await fetch('https://helper.testnet.near.org/account', {
  method: 'POST', headers: { 'Content-Type': 'application/json' },
  body: JSON.stringify({ newAccountId: id, newAccountPublicKey: kp.getPublicKey().toString() }),
});
if (!res.ok) { console.log('helper refused:', res.status, (await res.text()).slice(0, 120)); process.exit(1); }
writeFileSync(process.env.OUT, JSON.stringify({ account: id, key: kp.toString() }));
console.log('created', id);
