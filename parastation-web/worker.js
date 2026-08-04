/*
 * @file /parastation-web/worker.js
 * @brief
 * Runs inside the dedicaated worker. loads the wasm module and translates postMessage events from the main thread into
 * calls on the Rust WebRunner.
 *
 * Message protocol (main thread -> worker):
 *   { type: 'init', canvas: OffscreenCanvas }
 *   { type: 'load_bios', bytes: Uint8Array }
 *   { type: 'load_disc', cueContent: string, binFiles: File[] }
 *   { type: 'key_down', code: string }
 *   { type: 'key_up', code: string }
 *
 * Message protocol (worker -> main thread):
 *   { type: 'log', payload: string }
 *   { type: 'audio', payload: Float32Array }
 * -----
 */

import init, { WebRunner } from './pkg/parastation_web.js';

let runner = null;
const CYCLES_PER_FRAME = 564480; // 33_868_800 Hz / 60fps

function frameLoop() {
    if (runner) {
        runner.tick_frame(CYCLES_PER_FRAME);
        drainAndSendAudio();
    }
    self.requestAnimationFrame(frameLoop);
}

// Pull audio from the Rust side and send it to the main thread periodically. Called once per tick_frame message
function drainAndSendAudio() {
    if (!runner) return;
    const samples = runner.drain_audio(4096);
    if (samples.length > 0) {
        self.postMessage({ type: 'audio', payload: samples }, [samples.buffer]);
    }
}

self.onmessage = async (e) => {
    const msg = e.data;

    switch (msg.type) {
        case 'init': {
            await init();
            runner = new WebRunner(msg.canvas);
            self.postMessage({ type: 'log', payload: 'Worker initialized, WebRunner created' });
            self.requestAnimationFrame(frameLoop); // start the loop once, here
            break;
        }

        case 'load_bios': {
            if (!runner) break;
            runner.load_bios(msg.bytes);
            self.postMessage({ type: 'log', payload: 'BIOS loaded' });
            break;
        }

        case 'load_disc': {
            if (!runner) break;
            const binFileMap = new Map();
            for (const file of msg.binFiles) {
                binFileMap.set(file.name, file);
            }
            runner.insert_disc(msg.cueContent, binFileMap);
            self.postMessage({ type: 'log', payload: 'Disc loaded' });
            break;
        }

        case 'tick_frame': {
            if (!runner) break;
            runner.tick_frame(msg.cycles);
            drainAndSendAudio();
            break;
        }

        // TODO keyboard input
        case 'key_down':
        case 'key_up': {
            break;
        }

        default:
            self.postMessage({ type: 'log', payload: `Unknown message type: ${msg.type}` });
    }
};