// Browser storage: the wallet JSON and signer choice in localStorage,
// proving keys (a few megabytes each) in IndexedDB.

const PREFIX = 'antumbra.';

export const local = {
  get(key: string): string | null {
    try { return localStorage.getItem(PREFIX + key); } catch { return null; }
  },
  set(key: string, value: string) {
    try { localStorage.setItem(PREFIX + key, value); } catch { /* private mode */ }
  },
  del(key: string) {
    try { localStorage.removeItem(PREFIX + key); } catch { /* ignore */ }
  },
};

function openDb(): Promise<IDBDatabase> {
  return new Promise((resolve, reject) => {
    const req = indexedDB.open('antumbra', 1);
    req.onupgradeneeded = () => req.result.createObjectStore('proving-keys');
    req.onsuccess = () => resolve(req.result);
    req.onerror = () => reject(req.error);
  });
}

export async function getProvingKey(name: string): Promise<Uint8Array | null> {
  try {
    const db = await openDb();
    return await new Promise((resolve, reject) => {
      const req = db.transaction('proving-keys').objectStore('proving-keys').get(name);
      req.onsuccess = () => resolve(req.result ? new Uint8Array(req.result) : null);
      req.onerror = () => reject(req.error);
    });
  } catch {
    return null;
  }
}

export async function putProvingKey(name: string, bytes: Uint8Array): Promise<void> {
  try {
    const db = await openDb();
    await new Promise<void>((resolve, reject) => {
      const tx = db.transaction('proving-keys', 'readwrite');
      tx.objectStore('proving-keys').put(bytes, name);
      tx.oncomplete = () => resolve();
      tx.onerror = () => reject(tx.error);
    });
  } catch { /* best effort */ }
}
