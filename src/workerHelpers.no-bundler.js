/*
 * Copyright 2022 Google Inc. All Rights Reserved.
 * Licensed under the Apache License, Version 2.0 (the "License");
 * you may not use this file except in compliance with the License.
 * You may obtain a copy of the License at
 *     http://www.apache.org/licenses/LICENSE-2.0
 * Unless required by applicable law or agreed to in writing, software
 * distributed under the License is distributed on an "AS IS" BASIS,
 * WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express or implied.
 * See the License for the specific language governing permissions and
 * limitations under the License.
 */

// This file is kept similar to workerHelpers.js, but intended to be used in
// a bundlerless ES module environment (which has a few differences).

const isDedicatedWorker =
  typeof DedicatedWorkerGlobalScope !== 'undefined' &&
  self instanceof DedicatedWorkerGlobalScope;

function waitForMsgType(target, type, { timeout = 30000 } = {}) {
  const types = Array.isArray(type) ? type : [type];
  return new Promise((resolve, reject) => {
    const timer = setTimeout(() => {
      cleanup();
      reject(new Error(`Timed out waiting for ${type}`));
    }, timeout);

    function cleanup() {
      clearTimeout(timer);
      target.removeEventListener('message', onMsg);
      if (target.removeEventListener) {
        target.removeEventListener('error', onError);
        target.removeEventListener('messageerror', onMessageError);
      }
    }

    function onMsg({ data }) {
      if (!data || !types.includes(data.type)) return;
      cleanup();
      resolve(data);
    }

    function onError(event) {
      cleanup();
      reject(event.error || new Error(event.message || 'Worker failed'));
    }

    function onMessageError(event) {
      cleanup();
      reject(new Error(`Worker messageerror: ${event}`));
    }

    target.addEventListener('message', onMsg);
    if (target.addEventListener) {
      target.addEventListener('error', onError, { once: true });
      target.addEventListener('messageerror', onMessageError, { once: true });
    }
  });
}

// We need to wait for a specific message because this file is used both
// as a Worker and as a regular script, so it might receive unrelated
// messages on the page.
if (isDedicatedWorker && self.name === 'wasm_bindgen_worker') {
  waitForMsgType(self, 'wasm_bindgen_worker_init').then(async data => {
    let pkg;
    try {
      pkg = await import(data.mainJS);
      pkg.initSync(data.init);
    } catch (err) {
      postMessage({ type: 'wasm_bindgen_worker_error', error: err.message || String(err) });
      return;
    }
    postMessage({ type: 'wasm_bindgen_worker_ready' });
    pkg.wbg_rayon_start_worker(data.receiver);
  });
}

let initThreadPoolPromise;
let rayonWorkers = [];

export async function startWorkers(module, memory, builder) {
  if (initThreadPoolPromise) return initThreadPoolPromise;

  initThreadPoolPromise = startWorkersInner(module, memory, builder).catch(err => {
    initThreadPoolPromise = undefined;
    throw err;
  });

  return initThreadPoolPromise;
}

async function startWorkersInner(module, memory, builder) {
  const workerInit = {
    type: 'wasm_bindgen_worker_init',
    init: { module, memory },
    receiver: builder.receiver(),
    mainJS: builder.mainJS()
  };

  // Self-spawn into new Workers.
  // The script is fetched as a blob so it works even if this script is
  // hosted remotely (e.g. on a CDN). This avoids a cross-origin
  // security error.
  const response = await fetch(import.meta.url);
  if (!response.ok) {
    throw new Error(`Failed to fetch worker helper: ${response.status}`);
  }
  const scriptBlob = await response.blob();
  const workerUrl = URL.createObjectURL(scriptBlob);

  try {
    const workers = await Promise.all(
      Array.from({ length: builder.numThreads() }, async () => {
        const worker = new Worker(workerUrl, {
          type: 'module',
          name: 'wasm_bindgen_worker'
        });

        try {
          worker.postMessage(workerInit);
          const data = await waitForMsgType(worker, [
            'wasm_bindgen_worker_ready',
            'wasm_bindgen_worker_error'
          ]);
          if (data.type === 'wasm_bindgen_worker_error') {
            throw new Error(data.error || 'Worker initialization failed');
          }
          return worker;
        } catch (err) {
          worker.terminate();
          throw err;
        }
      })
    );

    try {
      builder.build();
    } catch (err) {
      for (const worker of workers) worker.terminate();
      throw err;
    }

    rayonWorkers.push(...workers);
  } finally {
    URL.revokeObjectURL(workerUrl);
  }
}
