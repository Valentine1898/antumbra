import { defineConfig } from 'vite';

export default defineConfig({
  build: { target: 'es2022' },
  worker: { format: 'es' },
  optimizeDeps: { exclude: ['antumbra-wasm'] },
  server: {
    port: 5173,
    strictPort: true,
    // The testnet helper (account faucet) is called directly in production; in dev it
    // goes through this proxy so a missing CORS header cannot block the demo.
    proxy: { '/helper': { target: 'https://helper.testnet.near.org', changeOrigin: true, rewrite: (p) => p.replace(/^\/helper/, '') } },
  },
});
