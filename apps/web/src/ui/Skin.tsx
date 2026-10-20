import { useEffect, useRef } from "react";

/** Renders the front view of a 64×64 Minecraft skin (head, body, arms, legs). */
export function SkinPreview({ src, scale = 6 }: { src: string | null; scale?: number }) {
  const ref = useRef<HTMLCanvasElement>(null);
  useEffect(() => {
    const cv = ref.current;
    if (!cv) return;
    const ctx = cv.getContext("2d")!;
    ctx.imageSmoothingEnabled = false;
    ctx.clearRect(0, 0, cv.width, cv.height);
    if (!src) return;
    const img = new Image();
    img.onload = () => {
      const s = scale;
      const part = (sx: number, sy: number, w: number, h: number, dx: number, dy: number) => ctx.drawImage(img, sx, sy, w, h, dx * s, dy * s, w * s, h * s);
      part(8, 8, 8, 8, 4, 0); // head
      part(40, 8, 8, 8, 4, 0); // hat layer
      part(20, 20, 8, 12, 4, 8); // body
      part(44, 20, 4, 12, 0, 8); // right arm
      img.height === 64 ? part(36, 52, 4, 12, 12, 8) : part(44, 20, 4, 12, 12, 8); // left arm
      part(4, 20, 4, 12, 4, 20); // right leg
      img.height === 64 ? part(20, 52, 4, 12, 8, 20) : part(4, 20, 4, 12, 8, 20); // left leg
    };
    img.src = src;
  }, [src, scale]);
  return <canvas ref={ref} width={16 * scale} height={32 * scale} className="pixel rounded-xl bg-panel2" aria-label="Skin preview" />;
}
