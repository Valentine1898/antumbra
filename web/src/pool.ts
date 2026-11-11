/// Which pool the page follows. `?pool=tee` selects the second pool, whose
/// epochs are cleared inside a TEE; anything else is the pool in deployment.json.
export const POOL_VARIANT: '' | 'tee' =
  new URLSearchParams(globalThis.location?.search ?? '').get('pool') === 'tee' ? 'tee' : '';

/// The deployment file of the pool the page follows.
export const DEPLOYMENT_FILE = POOL_VARIANT === 'tee' ? 'deployment-tee.json' : 'deployment.json';
