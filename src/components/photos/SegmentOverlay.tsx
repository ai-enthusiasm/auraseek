/**
 * SegmentOverlay — canvas-based bbox overlay renderer.
 *
 * Paints bounding boxes + labels + crisp borders on a canvas that
 * sits on top of an <img> rendered with object-cover or object-contain.
 *
 * Works for both object detection and face detection.
 */

import { useRef, useEffect } from "react";
import type { DetectedObject, DetectedFace } from "@/types/photo.type";

export interface SegmentOverlayProps {
  /** Detected objects */
  detectedObjects?: DetectedObject[];
  /** Detected faces — highlighted using bbox fill */
  detectedFaces?: DetectedFace[];
  /** Original image width in pixels */
  imgNaturalW: number;
  /** Original image height in pixels */
  imgNaturalH: number;
  /** Rendered display width (clientWidth of the <img>) */
  displayW: number;
  /** Rendered display height (clientHeight of the <img>) */
  displayH: number;
  /** CSS object-fit mode — determines coordinate mapping */
  objectFit?: "cover" | "contain";
  /** Whether to draw face bbox rectangles (default: true) */
  showFaces?: boolean;
  /** Whether to draw object/face labels + confidence (default: false) */
  showLabels?: boolean;
  /** Current zoom/view scale — use this to adjust stroke/font size to stay crisp */
  viewScale?: number;
  /** Whether to render bbox rectangles (objects + faces) (default: true) */
  showBoxes?: boolean;
  /** If set, only this object index will be highlighted (label) */
  activeObjectIndex?: number | null;
  /** If true, chỉ vẽ mask/label cho activeObjectIndex (khi chưa có index thì không vẽ gì) */
  onlyActive?: boolean;
  /** If set, highlight this face ID permanently */
  activeFaceId?: string;
  /** If true, draw all faces. If false and activeFaceId is set, only draw the active face. */
  showAllFaces?: boolean;
  /** If set, highlight this object class name permanently */
  activeClassName?: string;
}

// Fill + border RGBA pairs for up to 6 objects
const PALETTE: [number, number, number][] = [
  [34,  211, 238], // cyan
  [251, 191,  36], // amber
  [ 74, 222, 128], // green
  [248, 113, 113], // red
  [192, 132, 252], // purple
  [251, 146,  60], // orange
];
const FACE_RGB: [number, number, number] = [167, 139, 250]; // violet

export function SegmentOverlay({
  detectedObjects = [],
  detectedFaces   = [],
  imgNaturalW,
  imgNaturalH,
  displayW,
  displayH,
  objectFit  = "cover",
  showFaces  = true,
  showLabels = false,
  viewScale  = 1,
  showBoxes  = true,
  activeObjectIndex = null,
  onlyActive = false,
  activeFaceId,
  showAllFaces = true,
  activeClassName,
}: SegmentOverlayProps) {
  const canvasRef = useRef<HTMLCanvasElement>(null);

  useEffect(() => {
    const canvas = canvasRef.current;
    if (!canvas || displayW === 0 || displayH === 0 || imgNaturalW === 0 || imgNaturalH === 0) return;

    canvas.width  = displayW;
    canvas.height = displayH;
    const ctx = canvas.getContext("2d");
    if (!ctx) return;
    ctx.clearRect(0, 0, displayW, displayH);

    // ── Coordinate transform ─────────────────────────────────────────
    let scaleX: number, scaleY: number, cropX: number, cropY: number;

    if (objectFit === "cover") {
      const s = Math.max(displayW / imgNaturalW, displayH / imgNaturalH);
      scaleX  = scaleY = s;
      cropX   = (imgNaturalW * s - displayW) / 2;
      cropY   = (imgNaturalH * s - displayH) / 2;
    } else {
      // contain: letterboxed
      const s = Math.min(displayW / imgNaturalW, displayH / imgNaturalH);
      scaleX  = scaleY = s;
      cropX   = -(displayW - imgNaturalW * s) / 2;  // negative = padding
      cropY   = -(displayH - imgNaturalH * s) / 2;
    }

    // ── 1. Map objects to indexes and colors ──────────────────────────
    const masks = detectedObjects.map((obj, i) => ({
      index: i,
      obj,
      rgb: PALETTE[i % PALETTE.length],
    }));

    const filteredMasks = activeClassName
      ? masks.filter(m => m.obj.class_name.toLowerCase() === activeClassName.toLowerCase())
      : masks;

    const hasActive = typeof activeObjectIndex === "number";
    const masksToRender = onlyActive
      ? (hasActive ? filteredMasks.filter(m => m.index === activeObjectIndex) : [])
      : (hasActive ? filteredMasks.filter(m => m.index === activeObjectIndex) : filteredMasks);

    // ── 2. Object bbox rectangles (for all objects) ──────────────────
    if (showBoxes) {
      const boxesToRender = activeClassName
        ? masks.filter(m => m.obj.class_name.toLowerCase() === activeClassName.toLowerCase())
        : (activeFaceId ? [] : masksToRender);

      boxesToRender.forEach(({ obj, rgb }) => {
        const x = obj.bbox.x * scaleX - cropX;
        const y = obj.bbox.y * scaleY - cropY;
        const w = obj.bbox.w * scaleX;
        const h = obj.bbox.h * scaleY;

        ctx.strokeStyle = `rgba(${rgb[0]},${rgb[1]},${rgb[2]},0.95)`;
        ctx.lineWidth   = Math.max(1, 2 / viewScale);
        ctx.strokeRect(x, y, w, h);
      });
    }

    // ── 3. Face bbox fills (violet) ───────────────────────────────────
    if (showFaces && showBoxes && !activeClassName) {
      for (const face of detectedFaces) {
        const isActive = activeFaceId === face.face_id;
        if (activeFaceId && !isActive && !showAllFaces) {
          continue;
        }

        const x = face.bbox.x * scaleX - cropX;
        const y = face.bbox.y * scaleY - cropY;
        const w = face.bbox.w * scaleX;
        const h = face.bbox.h * scaleY;

        ctx.fillStyle   = `rgba(${FACE_RGB[0]},${FACE_RGB[1]},${FACE_RGB[2]},0.35)`;
        ctx.fillRect(x, y, w, h);
        ctx.strokeStyle = `rgba(${FACE_RGB[0]},${FACE_RGB[1]},${FACE_RGB[2]},0.95)`;
        ctx.lineWidth   = 2;
        ctx.strokeRect(x, y, w, h);
      }
    }

    // ── 4. Labels ─────────────────────────────────────────────────────
    if (showLabels) {
      const label = (text: string, bx: number, by: number, rgb: [number,number,number]) => {
        const fontSize = Math.max(6, 11 / viewScale);
        ctx.font = `bold ${fontSize}px system-ui, sans-serif`;
        const tw  = ctx.measureText(text).width;
        const pad = 4 / viewScale;
        const lh  = 17 / viewScale;
        const lx  = Math.max(0, Math.min(bx, displayW - tw - pad * 2));
        const ly  = by > lh + 2 / viewScale ? by - lh - 2 / viewScale : by + 2 / viewScale;
        ctx.fillStyle = `rgba(${rgb[0]},${rgb[1]},${rgb[2]},0.88)`;
        ctx.fillRect(lx, ly, tw + pad * 2, lh);
        ctx.fillStyle = "#fff";
        ctx.fillText(text, lx + pad, ly + lh - 4 / viewScale);
      };

      const labelMasks = activeClassName
        ? masks.filter(m => m.obj.class_name.toLowerCase() === activeClassName.toLowerCase())
        : (activeFaceId ? [] : (activeObjectIndex != null ? masksToRender : masks));

      labelMasks.forEach(({ obj, rgb }) => {
        label(
          `${obj.class_name} ${(obj.conf * 100).toFixed(0)}%`,
          obj.bbox.x * scaleX - cropX,
          obj.bbox.y * scaleY - cropY,
          rgb,
        );
      });

      if (showFaces && !activeClassName) {
        detectedFaces.forEach(face => {
          const isActive = activeFaceId === face.face_id;
          if (activeFaceId && !isActive && !showAllFaces) {
            return;
          }
          label(
            face.name ?? "Face",
            face.bbox.x * scaleX - cropX,
            face.bbox.y * scaleY - cropY,
            FACE_RGB,
          );
        });
      }
    }

  }, [
    detectedObjects,
    detectedFaces,
    imgNaturalW,
    imgNaturalH,
    displayW,
    displayH,
    objectFit,
    showFaces,
    showLabels,
    showBoxes,
    viewScale,
    activeObjectIndex,
    activeFaceId,
    showAllFaces,
    activeClassName,
  ]);

  return (
    <canvas
      ref={canvasRef}
      className="absolute inset-0 pointer-events-none"
      style={{ width: "100%", height: "100%" }}
    />
  );
}
