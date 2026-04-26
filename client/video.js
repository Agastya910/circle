// Browser-native video compression using canvas.captureStream + MediaRecorder.
// Re-encodes the input file to WebM (VP9 if available, else default) at the
// requested bitrate with a max-width cap.
//
// Returns: Promise<Blob>
// Rejects with: string error message
window.compressVideo = function (file, maxDurationSec, targetBitrate, maxWidth = 640) {
  return new Promise((resolve, reject) => {
    const video = document.createElement("video");
    video.muted = true;
    video.playsInline = true;
    video.preload = "auto";
    video.src = URL.createObjectURL(file);

    const cleanup = () => {
      try { URL.revokeObjectURL(video.src); } catch (_) {}
    };

    video.onerror = () => { cleanup(); reject("could not load video"); };

    video.onloadedmetadata = async () => {
      const duration = video.duration;
      if (!isFinite(duration)) { cleanup(); reject("could not read video duration"); return; }
      if (duration > maxDurationSec + 0.25) {
        cleanup();
        reject(`video too long (${duration.toFixed(1)}s); max ${maxDurationSec}s`);
        return;
      }

      const srcW = video.videoWidth;
      const srcH = video.videoHeight;
      if (!srcW || !srcH) { cleanup(); reject("invalid video dimensions"); return; }

      const targetW = Math.min(srcW, maxWidth);
      const scale = targetW / srcW;
      const targetH = Math.round(srcH * scale);

      const canvas = document.createElement("canvas");
      canvas.width = targetW;
      canvas.height = targetH;
      const ctx = canvas.getContext("2d");

      const fps = 30;
      const videoStream = canvas.captureStream(fps);

      // Mix in audio from the source video if available.
      let combinedStream = videoStream;
      let audioCtx = null;
      try {
        audioCtx = new (window.AudioContext || window.webkitAudioContext)();
        const src = audioCtx.createMediaElementSource(video);
        const dest = audioCtx.createMediaStreamDestination();
        src.connect(dest);
        // Don't route to speakers; we just want the stream.
        combinedStream = new MediaStream([
          ...videoStream.getVideoTracks(),
          ...dest.stream.getAudioTracks(),
        ]);
      } catch (e) {
        // No audio (e.g. autoplay policy, no track) — keep video-only.
        combinedStream = videoStream;
      }

      // Try mp4/H.264 first for Safari/iOS (which doesn't support webm in
      // MediaRecorder and can't play webm anyway). Fall back to webm on
      // Chromium/Firefox.
      const candidates = [
        "video/mp4;codecs=avc1.42E01E,mp4a.40.2",
        "video/mp4;codecs=h264,aac",
        "video/mp4",
        "video/webm;codecs=vp9,opus",
        "video/webm;codecs=vp8,opus",
        "video/webm;codecs=vp9",
        "video/webm;codecs=vp8",
        "video/webm",
      ];
      const mime = candidates.find((m) => MediaRecorder.isTypeSupported(m));
      if (!mime) { cleanup(); reject("no supported recorder format"); return; }

      let recorder;
      try {
        recorder = new MediaRecorder(combinedStream, {
          mimeType: mime,
          videoBitsPerSecond: targetBitrate,
          audioBitsPerSecond: 64_000,
        });
      } catch (e) {
        cleanup();
        reject("recorder init failed: " + e);
        return;
      }

      const chunks = [];
      recorder.ondataavailable = (e) => { if (e.data.size > 0) chunks.push(e.data); };
      recorder.onerror = (e) => { cleanup(); reject("recorder error: " + e.error); };
      recorder.onstop = () => {
        cleanup();
        resolve(new Blob(chunks, { type: mime }));
      };

      let raf = 0;
      const draw = () => {
        if (video.ended || video.paused) {
          if (recorder.state !== "inactive") recorder.stop();
          return;
        }
        ctx.drawImage(video, 0, 0, targetW, targetH);
        raf = requestAnimationFrame(draw);
      };

      video.onended = () => {
        cancelAnimationFrame(raf);
        if (recorder.state !== "inactive") recorder.stop();
      };

      recorder.start(100);
      try {
        await video.play();
        draw();
      } catch (e) {
        cleanup();
        reject("cannot play video: " + e);
      }
    };
  });
};
