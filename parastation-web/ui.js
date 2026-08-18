/*
 * @file /parastation-web/ui.js
 * @brief
 * Handles purely visual UI elements and user interactions, like the start screen, drawer, and file selection.
 * Dispatches events to main.js when the user is ready to start the emulator, and updates the UI based on messages 
 * from main.js.
 *
 * -----
 */

// Render scale presets
const scaleInput = document.getElementById('scaleInput');
document.querySelectorAll('.preset-btn').forEach(btn => {
  btn.addEventListener('click', () => { scaleInput.value = btn.dataset.scale; });
});

// BIOS file selection (start screen)
const biosInput = document.getElementById('bios-input');
const biosDrop = document.getElementById('biosDrop');
const startBtn = document.getElementById('startBtn');
let selectedBiosFile = null;

biosInput.addEventListener('change', () => {
  const file = biosInput.files[0];
  if (!file) return;
  selectedBiosFile = file;
  biosDrop.textContent = `${file.name}`;
  biosDrop.classList.add('loaded');
  startBtn.disabled = false;
  startBtn.textContent = 'Start';
});

// Start button: hands the selected BIOS file and scale to main.js, which will initialize the worker and start the
// emulator
const startScreen = document.getElementById('startScreen');
const startScreenWrapper = document.getElementById('startScreenWrapper');
const playScreen = document.getElementById('playScreen');

startBtn.addEventListener('click', () => {
  if (!selectedBiosFile || startBtn.disabled) return;

  const scaleRaw = parseInt(scaleInput.value, 10);
  const scale = Number.isInteger(scaleRaw) && scaleRaw > 0 ? scaleRaw : 1;

  startBtn.disabled = true;
  document.dispatchEvent(new CustomEvent('parastation-start', {
    detail: { biosFile: selectedBiosFile, scale }
  }));

  startScreen.style.display = 'none';
  startScreenWrapper.style.display = 'none';
  playScreen.classList.add('active');
});

// Drawer open/close toggle
const drawerHandle = document.getElementById('drawerHandle');
const drawer = document.getElementById('drawer');

function setDrawerOpen(open) {
  drawer.classList.toggle('open', open);
  drawerHandle.classList.toggle('open', open);
  drawerHandle.textContent = open ? '<' : '☰'; // Thanks for the hamburger icon claude
}

drawerHandle.addEventListener('click', () => {
  setDrawerOpen(!drawer.classList.contains('open'));
});

// Disc / memory card file-drop label text
const discInput = document.getElementById('disc-input');
const discDrop = document.getElementById('discDrop');
discInput.addEventListener('change', () => {
  const files = Array.from(discInput.files);
  const cue = files.find(f => f.name.toLowerCase().endsWith('.cue'));
  discDrop.textContent = cue ? `${cue.name}` : 'Load Disc (.cue/.bin)';
});

document.querySelectorAll('input[id^="memcard-input-"]').forEach((input) => {
    const dropLabel = document.getElementById(`memcardLoadDrop${parseInt(input.dataset.port, 10) + 1}`);
    input.addEventListener('change', () => {
        const file = input.files[0];
        dropLabel.textContent = file ? `${file.name}` : `Load Memory Card ${parseInt(input.dataset.port, 10) + 1}`;
    });
});