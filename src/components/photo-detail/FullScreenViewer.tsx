import { ArrowLeft, Info, Share2, Star, Trash2, Eye, EyeOff, ZoomIn, Plus, Minus, Undo, Scan } from "lucide-react";
import type { Photo } from "@/types/photo.type";
import { Button } from "@/components/ui/button";
import { Tooltip, TooltipTrigger, TooltipContent } from "@/components/ui/tooltip";
import { PhotoInfoPanel } from "./PhotoInfoPanel";
import { useState, useRef, useEffect } from "react";
import { AuraSeekApi } from "@/lib/api";

export function FullScreenViewer({ 
    photo, 
    onClose,
    isTrashMode = false,
    isHiddenMode = false,
}: { 
    photo: Photo, 
    onClose: () => void,
    isTrashMode?: boolean,
    isHiddenMode?: boolean,
}) {
    const [showInfo, setShowInfo] = useState(true);
    const [showBbox, setShowBbox] = useState(() => {
        const saved = localStorage.getItem(`auraseek_bbox_${photo.id}`);
        return saved === "true"; // Default to false
    });
    const [isFavorite, setIsFavorite] = useState(photo.favorite || false);
    const [isSharing, setIsSharing] = useState(false);

    useEffect(() => {
        const saved = localStorage.getItem(`auraseek_bbox_${photo.id}`);
        setShowBbox(saved === "true");
        setIsFavorite(photo.favorite || false);
    }, [photo.id, photo.favorite]);
    const imgRef = useRef<HTMLImageElement>(null);
    const containerRef = useRef<HTMLDivElement>(null);
    const [dimensions, setDimensions] = useState({ width: 0, height: 0 });

    // Zoom and Pan States
    const [scale, setScale] = useState(1);
    const [pan, setPan] = useState({ x: 0, y: 0 });
    const [isDragging, setIsDragging] = useState(false);
    const dragStart = useRef({ x: 0, y: 0 });
    const MIN_SCALE = 1;
    const MAX_SCALE = 5;

    // Cập nhật lại kích thước khi load ảnh xong hoặc resize cửa sổ
    useEffect(() => {
        const updateDimensions = () => {
            if (containerRef.current) {
                setDimensions({
                    width: containerRef.current.clientWidth,
                    height: containerRef.current.clientHeight,
                });
            }
        };

        window.addEventListener("resize", updateDimensions);
        updateDimensions();
        
        return () => window.removeEventListener("resize", updateDimensions);
    }, []);

    // Calculate actual rendered dimensions for clamping pan and minimap
    let renderedW = dimensions.width - 32;
    let renderedH = dimensions.height - 32;
    if (imgRef.current && dimensions.width && dimensions.height) {
        const natW = photo.width || imgRef.current.naturalWidth || 1;
        const natH = photo.height || imgRef.current.naturalHeight || 1;
        const imgAspect = natW / natH;
        const containerAspect = (dimensions.width - 32) / (dimensions.height - 32);
        if (imgAspect > containerAspect) {
            renderedW = dimensions.width - 32;
            renderedH = (dimensions.width - 32) / imgAspect;
        } else {
            renderedH = dimensions.height - 32;
            renderedW = (dimensions.height - 32) * imgAspect;
        }
    }

    const clampPan = (p: { x: number, y: number }, curScale: number) => {
        const maxX = Math.max(0, (renderedW * curScale - (dimensions.width - 32)) / 2);
        const maxY = Math.max(0, (renderedH * curScale - (dimensions.height - 32)) / 2);
        return {
            x: Math.min(Math.max(p.x, -maxX), maxX),
            y: Math.min(Math.max(p.y, -maxY), maxY)
        };
    };

    const handleWheel = (e: React.WheelEvent<HTMLDivElement>) => {
        if (!containerRef.current) return;
        
        // Cần stopPropagation nếu image pan bên scroll? Ở đây không cấp thiết, nhưng tính clamp cẩn thận.
        const zoomDelta = -e.deltaY * 0.005;
        let newScale = scale * Math.exp(zoomDelta);
        newScale = Math.max(MIN_SCALE, Math.min(MAX_SCALE, newScale));
        
        if (newScale === MIN_SCALE) {
            setScale(MIN_SCALE);
            setPan({ x: 0, y: 0 });
            return;
        }

        const rect = containerRef.current.getBoundingClientRect();
        const clientX = e.clientX - rect.left - rect.width / 2;
        const clientY = e.clientY - rect.top - rect.height / 2;

        const scaleRatio = newScale / scale;
        const newPanX = clientX - (clientX - pan.x) * scaleRatio;
        const newPanY = clientY - (clientY - pan.y) * scaleRatio;

        setScale(newScale);
        setPan(clampPan({ x: newPanX, y: newPanY }, newScale));
    };

    const handleDoubleClick = (e: React.MouseEvent) => {
        if (scale > 1) {
            setScale(1);
            setPan({ x: 0, y: 0 });
        } else {
            const newScale = 2.5;
            const rect = containerRef.current!.getBoundingClientRect();
            const clientX = e.clientX - rect.left - rect.width / 2;
            const clientY = e.clientY - rect.top - rect.height / 2;
            setScale(newScale);
            setPan(clampPan({ x: -clientX * 1.5, y: -clientY * 1.5 }, newScale));
        }
    };

    const handlePointerDown = (e: React.PointerEvent) => {
        if (scale === 1) return;
        setIsDragging(true);
        // Lưu lại offset tương tự như cách kéo CSS
        dragStart.current = { x: e.clientX - pan.x, y: e.clientY - pan.y };
        e.currentTarget.setPointerCapture(e.pointerId);
    };

    const handlePointerMove = (e: React.PointerEvent) => {
        if (!isDragging) return;
        const newPanX = e.clientX - dragStart.current.x;
        const newPanY = e.clientY - dragStart.current.y;
        setPan(clampPan({ x: newPanX, y: newPanY }, scale));
    };

    const handlePointerUp = (e: React.PointerEvent) => {
        setIsDragging(false);
        e.currentTarget.releasePointerCapture(e.pointerId);
    };

    const hasOverlays =
        (photo.detectedObjects && photo.detectedObjects.length > 0) ||
        (photo.detectedFaces && photo.detectedFaces.length > 0);

    const handleFavorite = async () => {
        try {
            const nextState = !isFavorite;
            setIsFavorite(nextState);
            // Dispatch event for optimistic update in parent (grid will update immediately)
            window.dispatchEvent(new CustomEvent("photo_toggle_favorite", { detail: { id: photo.id } }));
            
            await AuraSeekApi.toggleFavorite(photo.id);
        } catch (e) {
            console.error("Toggle favorite failed", e);
            setIsFavorite(!isFavorite); // Revert
            // Re-trigger global refresh on failure to ensure sync
            window.dispatchEvent(new Event("refresh_photos"));
        }
    };

    const handleShare = async () => {
        try {
            setIsSharing(true);
            const response = await fetch(photo.url);
            const blob = await response.blob();
            await navigator.clipboard.write([
                new ClipboardItem({
                    [blob.type]: blob
                })
            ]);
            // toast success would go here
        } catch (e) {
            console.error("Share failed", e);
        } finally {
            setIsSharing(false);
        }
    };

    const handleMoveToTrash = async () => {
        try {
            await AuraSeekApi.moveToTrash(photo.id);
            window.dispatchEvent(new Event("refresh_photos"));
            onClose();
        } catch (e) {
            console.error("Move to trash failed", e);
        }
    };

    const handleRestoreFromTrash = async () => {
        try {
            await AuraSeekApi.restoreFromTrash(photo.id);
            window.dispatchEvent(new Event("refresh_photos"));
            onClose();
        } catch (e) {
            console.error("Restore failed", e);
        }
    };

    const handleHide = async () => {
        try {
            await AuraSeekApi.hidePhoto(photo.id);
            window.dispatchEvent(new Event("refresh_photos"));
            onClose();
        } catch (e) {
            console.error("Hide failed", e);
        }
    };

    const handleUnhide = async () => {
        try {
            await AuraSeekApi.unhidePhoto(photo.id);
            window.dispatchEvent(new Event("refresh_photos"));
            onClose();
        } catch (e) {
            console.error("Unhide failed", e);
        }
    };

    return (
        <div className="fixed inset-0 z-50 flex bg-background w-full h-full text-foreground">

            {/* Left Pane (Image Viewer) */}
            <div className="relative flex-1 flex flex-col overflow-hidden bg-black transition-all">
                {/* Top Controls Overlay */}
                <div className="h-14 shrink-0 flex items-center justify-between px-2 bg-black text-white z-10">
                    <div className="flex items-center gap-2">
                        <Tooltip>
                            <TooltipTrigger asChild>
                                <Button variant="ghost" size="icon" onClick={onClose} className="rounded-full text-white/80 hover:text-white hover:bg-white/20">
                                    <ArrowLeft className="w-5 h-5" />
                                </Button>
                            </TooltipTrigger>
                            <TooltipContent side="bottom" className="text-xs">
                                <p>Quay lại</p>
                            </TooltipContent>
                        </Tooltip>
                    </div>
                    <div className="flex items-center gap-1">
                        {hasOverlays && (
                            <Tooltip>
                                <TooltipTrigger asChild>
                                    <Button
                                        variant="ghost"
                                        size="icon"
                                        onClick={() => {
                                            const newVal = !showBbox;
                                            setShowBbox(newVal);
                                            localStorage.setItem(`auraseek_bbox_${photo.id}`, newVal.toString());
                                        }}
                                        className={`rounded-full transition-colors ${showBbox ? "bg-white/20 text-white hover:bg-white/30" : "text-white/80 hover:text-white hover:bg-white/20"}`}
                                    >
                                        <Scan className="w-5 h-5" />
                                    </Button>
                                </TooltipTrigger>
                                <TooltipContent side="bottom" className="text-xs">
                                    <p>{showBbox ? "Ẩn khung AI" : "Hiện khung AI"}</p>
                                </TooltipContent>
                            </Tooltip>
                        )}

                        <Tooltip>
                            <TooltipTrigger asChild>
                                <Button
                                    variant="ghost"
                                    size="icon"
                                    className={`rounded-full transition-colors ${scale > 1 ? "bg-white/20 text-white hover:bg-white/30" : "text-white/80 hover:text-white hover:bg-white/20"}`}
                                    onClick={(e) => handleDoubleClick(e as any)}
                                >
                                    <ZoomIn className="w-5 h-5" />
                                </Button>
                            </TooltipTrigger>
                            <TooltipContent side="bottom" className="text-xs">
                                <p>Thu phóng ({Math.round(scale * 100)}%)</p>
                            </TooltipContent>
                        </Tooltip>

                        {/* Common actions based on mode */}
                        {isTrashMode ? (
                            <>
                                <Tooltip>
                                    <TooltipTrigger asChild>
                                        <Button
                                            variant="ghost"
                                            size="icon"
                                            className="rounded-full hover:bg-white/10 text-white"
                                            onClick={handleRestoreFromTrash}
                                        >
                                            <Undo className="w-5 h-5" />
                                        </Button>
                                    </TooltipTrigger>
                                    <TooltipContent side="bottom" className="text-xs">Khôi phục</TooltipContent>
                                </Tooltip>
                            </>
                        ) : isHiddenMode ? (
                            <>
                                <Tooltip>
                                    <TooltipTrigger asChild>
                                        <Button
                                            variant="ghost"
                                            size="icon"
                                            className="rounded-full hover:bg-white/10 text-white"
                                            onClick={handleUnhide}
                                        >
                                            <Eye className="w-5 h-5" />
                                        </Button>
                                    </TooltipTrigger>
                                    <TooltipContent side="bottom" className="text-xs">Bỏ ẩn</TooltipContent>
                                </Tooltip>
                            </>
                        ) : (
                            <>
                                <Tooltip>
                                    <TooltipTrigger asChild>
                                        <Button
                                            variant="ghost"
                                            size="icon"
                                            className={`rounded-full transition-colors ${isFavorite ? "text-yellow-400 bg-yellow-400/10 hover:bg-yellow-400/20" : "text-white hover:bg-white/10"}`}
                                            onClick={handleFavorite}
                                        >
                                            <Star className={`w-5 h-5 ${isFavorite ? "fill-current" : ""}`} />
                                        </Button>
                                    </TooltipTrigger>
                                    <TooltipContent side="bottom" className="text-xs">Yêu thích</TooltipContent>
                                </Tooltip>

                                <Tooltip>
                                    <TooltipTrigger asChild>
                                        <Button
                                            variant="ghost"
                                            size="icon"
                                            className="rounded-full hover:bg-white/10 text-white"
                                            onClick={handleHide}
                                        >
                                            <EyeOff className="w-5 h-5" />
                                        </Button>
                                    </TooltipTrigger>
                                    <TooltipContent side="bottom" className="text-xs">Ẩn ảnh</TooltipContent>
                                </Tooltip>

                                <Tooltip>
                                    <TooltipTrigger asChild>
                                        <Button
                                            variant="ghost"
                                            size="icon"
                                            className="rounded-full hover:bg-white/10 text-white"
                                            onClick={handleShare}
                                            disabled={isSharing}
                                        >
                                            <Share2 className="w-5 h-5" />
                                        </Button>
                                    </TooltipTrigger>
                                    <TooltipContent side="bottom" className="text-xs">Chia sẻ</TooltipContent>
                                </Tooltip>

                                <Tooltip>
                                    <TooltipTrigger asChild>
                                        <Button
                                            variant="ghost"
                                            size="icon"
                                            className="rounded-full hover:bg-destructive/20 text-white hover:text-destructive-foreground hover:bg-destructive"
                                            onClick={handleMoveToTrash}
                                        >
                                            <Trash2 className="w-5 h-5" />
                                        </Button>
                                    </TooltipTrigger>
                                    <TooltipContent side="bottom" className="text-xs">Xóa vào thùng rác</TooltipContent>
                                </Tooltip>
                            </>
                        )}

                        <div className="h-6 w-px bg-white/20 mx-1" />

                        <Tooltip>
                            <TooltipTrigger asChild>
                                <Button
                                    variant="ghost"
                                    size="icon"
                                    onClick={() => setShowInfo(!showInfo)}
                                    className={`rounded-full transition-colors ${showInfo ? "bg-white/20 text-white hover:bg-white/30" : "text-white/80 hover:text-white hover:bg-white/20"}`}
                                >
                                    <Info className="w-5 h-5" />
                                </Button>
                            </TooltipTrigger>
                            <TooltipContent side="bottom" className="text-xs">
                                <p>Thông tin</p>
                            </TooltipContent>
                        </Tooltip>
                    </div>
                </div>

                {/* The Image */}
                <div 
                    ref={containerRef} 
                    className="flex-1 flex items-center justify-center p-4 relative overflow-hidden select-none outline-none"
                    onWheel={handleWheel}
                    onDoubleClick={handleDoubleClick}
                    onPointerDown={handlePointerDown}
                    onPointerMove={handlePointerMove}
                    onPointerUp={handlePointerUp}
                    onPointerCancel={handlePointerUp}
                    style={{ cursor: scale > 1 ? (isDragging ? 'grabbing' : 'grab') : 'default' }}
                >
                    <div 
                        className="relative w-full h-full flex justify-center items-center pointer-events-none"
                        style={{
                            transform: `translate(${pan.x}px, ${pan.y}px) scale(${scale})`,
                            transformOrigin: "center center",
                            transition: isDragging ? "none" : "transform 0.1s ease-out, scale 0.1s ease-out"
                        }}
                    >
                        <img
                            ref={imgRef}
                            src={photo.url}
                            alt="View"
                            className="max-w-full max-h-full object-contain pointer-events-auto shadow-2xl"
                            draggable={false}
                            onLoad={() => {
                                // Trigger re-render to calculate bbox when image loads
                                setDimensions({
                                    width: containerRef.current?.clientWidth || 0,
                                    height: containerRef.current?.clientHeight || 0,
                                });
                            }}
                        />
                        {/* Overlay Bbox */}
                        {showBbox && hasOverlays && dimensions.width > 0 && (
                            <FullScreenBboxOverlay 
                                photo={photo} 
                                imgRef={imgRef} 
                                containerWidth={dimensions.width - 32} 
                                containerHeight={dimensions.height - 32} 
                                viewScale={scale}
                            />
                        )}
                    </div>
                    
                    {/* Minimap Overlay (Top Right inside Image container) */}
                    {scale > 1 && (
                        <div className="absolute top-4 right-4 z-20 flex flex-col items-end gap-2 text-white pointer-events-auto">
                            <div className="bg-black/60 backdrop-blur-md border border-white/20 rounded-xl p-2 shadow-2xl relative">
                                <div className="relative" style={{ width: 120, height: 120 * (renderedH / renderedW) }}>
                                    <img src={photo.url} className="w-full h-full object-contain opacity-60 rounded-sm" />
                                    {/* Viewport Box */}
                                    <div 
                                        className="absolute border border-white/80 bg-white/20 rounded-[2px]"
                                        style={{
                                            width: `${Math.min(100, ((dimensions.width - 32) / (renderedW * scale)) * 100)}%`,
                                            height: `${Math.min(100, ((dimensions.height - 32) / (renderedH * scale)) * 100)}%`,
                                            left: `${Math.max(0, Math.min(100, (((renderedW * scale - (dimensions.width - 32)) / 2 - pan.x) / (renderedW * scale)) * 100))}%`,
                                            top: `${Math.max(0, Math.min(100, (((renderedH * scale - (dimensions.height - 32)) / 2 - pan.y) / (renderedH * scale)) * 100))}%`,
                                            transition: isDragging ? "none" : "all 0.1s ease-out"
                                        }}
                                    />
                                </div>
                            </div>
                            <div className="bg-black/60 backdrop-blur-md border border-white/20 rounded-full flex items-center p-1 text-white gap-2 shadow-2xl">
                                <Button 
                                    variant="ghost" size="icon" className="w-7 h-7 rounded-full hover:bg-white/20" 
                                    onClick={() => {
                                        const ns = Math.max(MIN_SCALE, scale - 0.5);
                                        setScale(ns);
                                        setPan(clampPan(pan, ns));
                                    }}>
                                    <Minus className="w-4 h-4" />
                                </Button>
                                <span className="text-xs font-medium w-10 text-center">{Math.round(scale * 100)}%</span>
                                <Button 
                                    variant="ghost" size="icon" className="w-7 h-7 rounded-full hover:bg-white/20" 
                                    onClick={() => setScale(s => Math.min(MAX_SCALE, s + 0.5))}>
                                    <Plus className="w-4 h-4" />
                                </Button>
                            </div>
                        </div>
                    )}
                </div>
            </div>

            {/* Right Pane (Info Side Panel) */}
            {showInfo && (
                <div className="w-[360px] md:w-[400px] shrink-0 border-l border-border/20 bg-background flex flex-col h-full overflow-hidden transition-all shadow-xl">
                    <div className="h-14 flex items-center px-4 border-b border-border/10">
                        <h2 className="text-lg font-medium tracking-tight">Thông tin</h2>
                    </div>
                    <div className="flex-1 overflow-y-auto">
                        <PhotoInfoPanel photo={photo} />
                    </div>
                </div>
            )}

        </div>
    );
}

function FullScreenBboxOverlay({
    photo,
    imgRef,
    containerWidth,
    containerHeight,
    viewScale = 1
}: {
    photo: Photo;
    imgRef: React.RefObject<HTMLImageElement | null>;
    containerWidth: number;
    containerHeight: number;
    viewScale?: number;
}) {
    const img = imgRef.current;
    if (!img || !img.naturalWidth || !img.naturalHeight) return null;

    const imgW = photo.width || img.naturalWidth || 1;
    const imgH = photo.height || img.naturalHeight || 1;

    const imgAspect = imgW / imgH;
    const containerAspect = containerWidth / containerHeight;

    let scale: number, renderedW: number, renderedH: number;

    // Ảnh hiển thị object-contain sẽ fit theo chiều nào?
    if (imgAspect > containerAspect) {
        // Ảnh fit theo chiều rộng (Width limits)
        renderedW = containerWidth;
        renderedH = containerWidth / imgAspect;
        scale = containerWidth / imgW;
    } else {
        // Ảnh fit theo chiều cao (Height limits)
        renderedH = containerHeight;
        renderedW = containerHeight * imgAspect;
        scale = containerHeight / imgH;
    }

    // Vì ảnh luôn được center (justify-center items-center)
    // Tính offset từ gốc của ảnh được render so với container (0, 0) relative parent
    // Parent relative đang bằng đúng (containerWidth, containerHeight)
    // img rendered ở giữa parent relative
    const offsetX = (containerWidth - renderedW) / 2;
    const offsetY = (containerHeight - renderedH) / 2;

    const boxes: React.ReactNode[] = [];

    photo.detectedObjects?.forEach((obj, i) => {
        const left = obj.bbox.x * scale + offsetX;
        const top = obj.bbox.y * scale + offsetY;
        const w = obj.bbox.w * scale;
        const h = obj.bbox.h * scale;
        boxes.push(
            <div
                key={`obj-${i}`}
                className="absolute z-20 pointer-events-none"
                style={{ 
                    left, top, width: w, height: h, 
                    border: `${Math.max(1, 2 / viewScale)}px solid #22d3ee`, 
                    borderRadius: 4 / viewScale 
                }}
            >
                <div className="absolute top-0 left-0 -translate-y-[100%] max-w-[200px] overflow-visible">
                    <span 
                        className="font-semibold bg-[#22d3ee]/90 text-black shadow-sm whitespace-nowrap block w-max"
                        style={{
                            fontSize: `${11 / viewScale}px`,
                            padding: `${2 / viewScale}px ${6 / viewScale}px`,
                            borderRadius: `${4 / viewScale}px ${4 / viewScale}px 0 0`,
                        }}
                    >
                        {obj.class_name} {(obj.conf * 100).toFixed(0)}%
                    </span>
                </div>
            </div>
        );
    });

    photo.detectedFaces?.forEach((face, i) => {
        const left = face.bbox.x * scale + offsetX;
        const top = face.bbox.y * scale + offsetY;
        const w = face.bbox.w * scale;
        const h = face.bbox.h * scale;
        boxes.push(
            <div
                key={`face-${i}`}
                className="absolute z-20 pointer-events-none"
                style={{ 
                    left, top, width: w, height: h, 
                    border: `${Math.max(1, 2 / viewScale)}px solid #a78bfa`, 
                    borderRadius: 4 / viewScale 
                }}
            >
                <div className="absolute top-0 left-0 -translate-y-[100%] max-w-[200px] overflow-visible">
                    <span 
                        className="font-semibold bg-[#a78bfa]/90 text-white shadow-sm whitespace-nowrap block w-max"
                        style={{
                            fontSize: `${11 / viewScale}px`,
                            padding: `${2 / viewScale}px ${6 / viewScale}px`,
                            borderRadius: `${4 / viewScale}px ${4 / viewScale}px 0 0`,
                        }}
                    >
                        {face.name || "Face"} {(face.conf * 100).toFixed(0)}%
                    </span>
                </div>
            </div>
        );
    });

    return (
        <div className="absolute top-0 left-0 right-0 bottom-0 pointer-events-none">
            {boxes}
        </div>
    );
}
