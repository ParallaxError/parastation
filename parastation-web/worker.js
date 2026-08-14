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
 *   { type: 'input_down', code: string }
 *   { type: 'input_up', code: string }
 *   { type: 'dump_vram' }
 *
 * Message protocol (worker -> main thread):
 *   { type: 'log', payload: string }
 *   { type: 'audio', payload: Float32Array }
 *   { type: 'vram_dump', 
 *      accurate: { 
 *          width: number, height: number, bytes: Uint8Array, sample: Uint8Array
 *      }, enhanced: { 
 *          width: number, height: number, bytes: Uint8Array, sample: Uint8Array
 *      } 
 *   }
 * -----
 */

import init, { WebRunner } from './pkg/parastation_web.js';

let runner = null;
const CYCLES_PER_SECOND = 33_868_800;
const PRESENT_INTERVAL_MS = 1000 / 60; // cap presentation to 60fps regardless of display refresh rate

let lastTimestamp = null;
let timeSinceLastPresent = 0;

const MAX_DELTA_MS = 100;

function frameLoop(timestamp) {
    if (runner) {
        if (lastTimestamp === null) {
            lastTimestamp = timestamp;
        }

        let deltaMs = timestamp - lastTimestamp;
        lastTimestamp = timestamp;
        deltaMs = Math.min(deltaMs, MAX_DELTA_MS);

        const cycles = Math.round((deltaMs / 1000) * CYCLES_PER_SECOND);
        if (cycles > 0) {
            const shouldPresent = timeSinceLastPresent >= PRESENT_INTERVAL_MS;
            runner.tick_frame(cycles);
            drainAndSendAudio();

            if (shouldPresent) {
                timeSinceLastPresent -= PRESENT_INTERVAL_MS;
                // handle cases where we overshot by more than one interval (avoid drift)
                if (timeSinceLastPresent >= PRESENT_INTERVAL_MS) {
                    timeSinceLastPresent = timeSinceLastPresent % PRESENT_INTERVAL_MS;
                }
            }
            timeSinceLastPresent += deltaMs;
        }
    }
    self.requestAnimationFrame(frameLoop);
}

self.requestAnimationFrame(frameLoop);

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

        case 'input_down': {
            if (!runner) break;
            runner.input_down(msg.id);
            break;
        }

        case 'input_up': {
            if (!runner) break;
            runner.input_up(msg.id);
            break;
        }

        case 'dump_vram': {
            if (!runner) break;

            const accurateBytes = runner.dump_accurate_vram();
            const accurateSampleBytes = runner.dump_accurate_sample();
            const accurateDims = runner.accurate_vram_dims();
            const enhancedBytes = runner.dump_enhanced_vram();
            const enhancedSampleBytes = runner.dump_enhanced_sample();
            const enhancedDims = runner.enhanced_vram_dims();

            self.postMessage({
                type: 'vram_dump',
                payload: {
                    accurate: accurateBytes ? { bytes: accurateBytes, sample: accurateSampleBytes, width: accurateDims[0], height: accurateDims[1] } : null,
                    enhanced: enhancedBytes ? { bytes: enhancedBytes, sample: enhancedSampleBytes, width: enhancedDims[0], height: enhancedDims[1] } : null,
                },
            });
            break;
        }

        default:
            self.postMessage({ type: 'log', payload: `Unknown message type: ${msg.type}` });
    }
};