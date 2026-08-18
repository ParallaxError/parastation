/*
 * @file /parastation-web/main.js
 * @brief
 * Runs on the main thread, owns the DOM, creates the Worker, forwards user input into the Worker with postMessage
 * and plays back audio samples sent by the worker.
 *
 * -----
 */

const logOutput = document.getElementById('log-output');
function log(msg, newline = true) {
    if (newline) logOutput.textContent += msg + '\n';
    else logOutput.textContent += msg;
    console.log(msg);
}

// Worker is created once the user finishes their initialisation process (bios, choosing canvas scale)
let worker = null;
let workerReady = false;

function handleWorkerMessage(e) {
    const { type, payload } = e.data;
    if (type === 'log') {
        log(payload);
    } else if (type == 'tty') {
        // Only add the TTY prefix if the message isn't a newline, to avoid cluttering the log with [TTY] lines
        if (payload === '\n') {
            log(payload, false);
        } else {
            log(`[TTY] ${payload}`, false);
        }
    } else if (type === 'audio') {
        playAudioSamples(payload);
    } else if (type === 'vram_dump') {
        if (payload.accurate) {
            downloadRgbaAsPng(payload.accurate.bytes, payload.accurate.width, payload.accurate.height, 'accurate_vram.png');
            downloadRgbaAsPng(payload.accurate.sample, payload.accurate.width, payload.accurate.height, 'accurate_vram_sample.png');
        }
        if (payload.enhanced) {
            downloadRgbaAsPng(payload.enhanced.bytes, payload.enhanced.width, payload.enhanced.height, 'enhanced_vram.png');
            downloadRgbaAsPng(payload.enhanced.sample, payload.enhanced.width, payload.enhanced.height, 'enhanced_vram_sample.png');
        }
    } else if (type === 'memcard_save') {
        downloadBytes(payload.bytes, `memcard_${payload.port}.mcd`);
        log(`Memory card ${payload.port} saved`);
    }
}

// Once the user selects a bios and scale we create the user here
document.addEventListener('parastation-start', async (e) => {
    if (worker) {
        log('Start already in progress or completed; ignoring duplicate start.');
        return;
    }
    const { biosFile, scale } = e.detail;

    // Native PS1 VRAM is 1024x512, canvas is scaled up by the user-selected factor (1x, 2x, 3x, etc)
    // In practice this is kind of eager: no game is gonna display the whole VRAM since the ps1 can't even do that
    // Might add debug options to do this though so Im doing this, I think browsers are optimised enough to not lag on
    // huge canvases
    const canvas = document.getElementById('ps1-canvas');
    canvas.width = Math.round(1024 * scale);
    canvas.height = Math.round(512 * scale);

    const offscreenCanvas = canvas.transferControlToOffscreen();

    worker = new Worker('worker.js', { type: 'module' });
    worker.onerror = (err) => {
        // Add an alert so the user knows something went wrong, since the console might not be open
        alert(`Worker error: ${err.message}`);
        log(`Worker error: ${err.message}`);
    };
    worker.onmessage = handleWorkerMessage;

    const ready = new Promise((resolve) => {
        worker.onmessage = (e) => {
            // Kind of a HACK
            handleWorkerMessage(e);
            if (e.data.type === 'log' && e.data.payload === 'Worker initialized, WebRunner created') {
                worker.onmessage = handleWorkerMessage; // hand off to the normal handler
                workerReady = true;
                resolve();
            }
        };
    });

    worker.postMessage(
        { type: 'init', canvas: offscreenCanvas, scale: scale },
        [offscreenCanvas]
    );
    await ready;

    const biosBytes = new Uint8Array(await biosFile.arrayBuffer());
    log(`Loading BIOS: ${biosFile.name} (${biosBytes.length} bytes)`);
    worker.postMessage({ type: 'load_bios', bytes: biosBytes }, [biosBytes.buffer]);
});

// Disc upload
// Sends the .cue file's text content plus the raw File handles for every .bin so the worker can read it with
// FileReaderSync
document.getElementById('disc-input').addEventListener('change', async (e) => {
    if (!workerReady) {
        log('Cannot load disc yet: emulator is still starting up.');
        return;
    }
    const files = Array.from(e.target.files);
    const cueFile = files.find(f => f.name.toLowerCase().endsWith('.cue'));
    if (!cueFile) {
        log('Did not find a .cue file in the selected files');
        return;
    }

    const cueContent = await cueFile.text();
    const binFiles = files.filter(f => f.name.toLowerCase().endsWith('.bin'));

    log(`Loading disc: ${cueFile.name} with ${binFiles.length} .bin file(s)`);
    worker.postMessage({ type: 'load_disc', cueContent, binFiles });
});

// Memory card upload/save
document.querySelectorAll('input[id^="memcard-input-"]').forEach((input) => {
    input.addEventListener('change', async (e) => {
        if (!workerReady) {
            log('Cannot load memory card yet: emulator is still starting up.');
            return;
        }
        const file = e.target.files[0];
        if (!file) return;
        const port = parseInt(input.dataset.port, 10);
        const bytes = new Uint8Array(await file.arrayBuffer());
        log(`Loading memory card ${port + 1}: ${file.name} (${bytes.length} bytes)`);
        worker.postMessage({ type: 'load_memcard', port: port + 1, bytes }, [bytes.buffer]);
    });
});

document.querySelectorAll('button[id^="memcardSaveBtn"]').forEach((btn) => {
    btn.addEventListener('click', () => {
        if (!workerReady) {
            log('Cannot save memory card yet: emulator is still starting up.');
            return;
        }
        const port = parseInt(btn.dataset.port, 10);
        worker.postMessage({ type: 'save_memcard', port: port + 1 });
    });
});

function downloadBytes(bytes, filename) {
    const blob = new Blob([bytes], { type: 'application/octet-stream' });
    const url = URL.createObjectURL(blob);
    const a = document.createElement('a');
    a.href = url;
    a.download = filename;
    a.click();
    URL.revokeObjectURL(url);
}

// Keyboard input forwarding to worker
document.querySelectorAll('.remap-key').forEach((keyEl) => {
    keyEl.addEventListener('click', () => {
        if (!workerReady) return;
        const buttonName = keyEl.dataset.button; // e.g. "Cross" — already on these elements
        const previousText = keyEl.textContent;
        keyEl.textContent = 'Press a key…';

        const captureKey = (e) => {
            e.preventDefault();
            worker.postMessage({ type: 'rebind_input', id: e.code, button: buttonName });
            keyEl.textContent = e.code;
            window.removeEventListener('keydown', captureKey);
        };
        window.addEventListener('keydown', captureKey);
    });
});

window.addEventListener('keydown', (e) => {
    if (!workerReady) return;
    worker.postMessage({ type: 'input_down', id: e.code });
});
window.addEventListener('keyup', (e) => {
    if (!workerReady) return;
    worker.postMessage({ type: 'input_up', id: e.code });
});

// Audio playback
let audioCtx = null;
let nextPlayTime = 0;

function playAudioSamples(samples) {
    if (samples.length === 0) return;

    // PS1 SPU audio is at 44.1KHz
    if (!audioCtx) {
        audioCtx = new (window.AudioContext || window.webkitAudioContext)({ sampleRate: 44100 });
        nextPlayTime = audioCtx.currentTime;
    }

    const frameCount = samples.length / 2; // interleaved L/R float32 samples
    const buffer = audioCtx.createBuffer(2, frameCount, 44100);
    const left = buffer.getChannelData(0);
    const right = buffer.getChannelData(1);
    for (let i = 0; i < frameCount; i++) {
        left[i] = samples[i * 2];
        right[i] = samples[i * 2 + 1];
    }

    const source = audioCtx.createBufferSource();
    source.buffer = buffer;
    source.connect(audioCtx.destination);

    if (nextPlayTime < audioCtx.currentTime) {
        nextPlayTime = audioCtx.currentTime;
    }
    source.start(nextPlayTime);
    nextPlayTime += frameCount / 44100;
}

// Debug: VRAM dump
function downloadRgbaAsPng(rgbaBytes, width, height, filename) {
    const canvas = document.createElement('canvas');
    canvas.width = width;
    canvas.height = height;
    const ctx = canvas.getContext('2d');
    const imageData = new ImageData(new Uint8ClampedArray(rgbaBytes), width, height);
    ctx.putImageData(imageData, 0, 0);

    canvas.toBlob((blob) => {
        const url = URL.createObjectURL(blob);
        const a = document.createElement('a');
        a.href = url;
        a.download = filename;
        a.click();
        URL.revokeObjectURL(url);
    }, 'image/png');
}

document.getElementById('dump-vram-btn').addEventListener('click', () => {
    if (!workerReady) {
        log('Cannot dump VRAM yet: emulator is still starting up.');
        return;
    }
    worker.postMessage({ type: 'dump_vram' });
});