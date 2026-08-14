/*
 * @file /parastation-web/main.js
 * @brief
 * Runs on the main thread, owns the DOM, creates the Worker, forwards user input into the Worker with postMessage
 * and plays back audio samples sent by the worker.
 *
 * -----
 */

const logOutput = document.getElementById('log-output');
function log(msg) {
    logOutput.textContent += msg + '\n';
    console.log(msg);
}

// Worker setup
const worker = new Worker('worker.js', { type: 'module' });

worker.onerror = (e) => {
    log(`WORKER ERROR: ${e.message} (${e.filename}:${e.lineno})`);
};

worker.onmessage = (e) => {
    const { type, payload } = e.data;
    if (type === 'log') {
        log(payload);
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
    }
};

// Send the canvas to the Worker as an OffscreenCanvas, so the Worker can render directly into it without needing to
// go through the main thread.
const canvas = document.getElementById('ps1-canvas');
const offscreenCanvas = canvas.transferControlToOffscreen();

// postMessage to transfer ownership and init
worker.postMessage(
    { type: 'init', canvas: offscreenCanvas },
    [offscreenCanvas]
);

// BIOS upload callback
document.getElementById('bios-input').addEventListener('change', async (e) => {
    const file = e.target.files[0];
    if (!file) return;
    const bytes = new Uint8Array(await file.arrayBuffer());
    log(`Loading BIOS: ${file.name} (${bytes.length} bytes)`);
    worker.postMessage({ type: 'load_bios', bytes }, [bytes.buffer]);
});

// Disc upload
// Sends the .cue file's TEXT content plus the raw File handles for every .bin so the worker can read it with
// FileReaderSync
document.getElementById('disc-input').addEventListener('change', async (e) => {
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

// Keyboard input forwarding to worker
window.addEventListener('keydown', (e) => {
    worker.postMessage({ type: 'input_down', id: e.code });
});
window.addEventListener('keyup', (e) => {
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

// Debug buttons
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
    worker.postMessage({ type: 'dump_vram' });
});