<script lang="ts">
  import { onDestroy, onMount } from 'svelte';
  import jsQR from 'jsqr';

  let { onfound }: { onfound: (text: string) => void } = $props();

  let video: HTMLVideoElement | undefined = $state();
  let stream: MediaStream | null = null;
  let error = $state<string | null>(null);
  let running = $state(false);
  let raf = 0;
  const canvas = document.createElement('canvas');
  const ctx = canvas.getContext('2d', { willReadFrequently: true })!;

  function decodeFrom(source: CanvasImageSource, w: number, h: number): string | null {
    const scale = Math.min(1, 900 / Math.max(w, h));
    canvas.width = Math.round(w * scale);
    canvas.height = Math.round(h * scale);
    ctx.drawImage(source, 0, 0, canvas.width, canvas.height);
    const img = ctx.getImageData(0, 0, canvas.width, canvas.height);
    return jsQR(img.data, img.width, img.height, { inversionAttempts: 'attemptBoth' })?.data ?? null;
  }

  function tickFrame() {
    if (!video || !running) return;
    if (video.readyState >= 2 && video.videoWidth) {
      const text = decodeFrom(video, video.videoWidth, video.videoHeight);
      if (text) {
        stop();
        onfound(text);
        return;
      }
    }
    raf = requestAnimationFrame(tickFrame);
  }

  async function start() {
    error = null;
    if (!navigator.mediaDevices?.getUserMedia) {
      error = "This device's browser can't open the camera here. Use “Choose a photo of the QR” instead.";
      return;
    }
    try {
      stream = await navigator.mediaDevices.getUserMedia({ video: { facingMode: 'environment' }, audio: false });
      video!.srcObject = stream;
      await video!.play();
      running = true;
      tickFrame();
    } catch {
      error = 'Camera access was blocked. Allow the camera, or choose a photo of the QR code instead.';
    }
  }

  function stop() {
    running = false;
    cancelAnimationFrame(raf);
    stream?.getTracks().forEach((t) => t.stop());
    stream = null;
  }

  async function fromFile(e: Event) {
    const file = (e.target as HTMLInputElement).files?.[0];
    if (!file) return;
    try {
      const bitmap = await createImageBitmap(file);
      const text = decodeFrom(bitmap, bitmap.width, bitmap.height);
      if (text) onfound(text);
      else error = "Couldn't find a QR code in that picture. Try a closer, sharper photo.";
    } catch {
      error = "Couldn't read that picture.";
    }
  }

  onMount(start);
  onDestroy(stop);
</script>

<div class="scanner">
  <!-- svelte-ignore a11y_media_has_caption -->
  <video bind:this={video} playsinline muted class:hidden={!running}></video>
  {#if running}<div class="frame" aria-hidden="true"></div>{/if}
  {#if error}<p class="small dim">{error}</p>{/if}
  <div class="row">
    {#if !running}<button class="btn small" onclick={start}>Use camera</button>{/if}
    <label class="btn small">
      Choose a photo of the QR
      <input type="file" accept="image/*" capture="environment" onchange={fromFile} hidden />
    </label>
  </div>
</div>

<style>
  .scanner {
    position: relative;
    display: grid;
    gap: 0.6rem;
  }
  video {
    width: 100%;
    max-height: 320px;
    object-fit: cover;
    border-radius: var(--r-md);
    background: #000;
  }
  .hidden {
    display: none;
  }
  .frame {
    position: absolute;
    top: 10%;
    left: 50%;
    width: min(55%, 220px);
    aspect-ratio: 1;
    transform: translateX(-50%);
    border: 2px solid var(--brass);
    border-radius: 12px;
    box-shadow: 0 0 0 999px rgba(0, 0, 0, 0.25);
    pointer-events: none;
  }
</style>
