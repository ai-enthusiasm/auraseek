import { ArrowLeft, Info, Share2, Star, Trash2 } from "lucide-react";
import type { Photo } from "@/types/photo.type";
import { Button } from "@/components/ui/button";
import { PhotoInfoPanel } from "./PhotoInfoPanel";
import { useState } from "react";

export function FullScreenViewer({ photo, onClose }: { photo: Photo, onClose: () => void }) {
    const [showInfo, setShowInfo] = useState(true);

    return (
        <div className="fixed inset-0 z-50 flex bg-background w-full h-full text-foreground">

            {/* Left Pane (Image Viewer) */}
            <div className="relative flex-1 flex flex-col overflow-hidden bg-black transition-all">
                {/* Top Controls Overlay */}
                <div className="absolute top-0 left-0 right-0 h-16 flex items-center justify-between px-2 z-10 bg-gradient-to-b from-black/60 to-transparent text-white opacity-0 hover:opacity-100 transition-opacity">
                    <div className="flex items-center gap-2">
                        <Button variant="ghost" size="icon" onClick={onClose} className="rounded-full text-white hover:bg-white/20">
                            <ArrowLeft className="w-5 h-5" />
                        </Button>
                    </div>
                    <div className="flex items-center gap-1">
                        <Button variant="ghost" size="icon" className="rounded-full text-white hover:bg-white/20">
                            <Share2 className="w-5 h-5" />
                        </Button>
                        <Button variant="ghost" size="icon" className="rounded-full text-white hover:bg-white/20">
                            <Star className={`w-5 h-5 ${photo.favorite ? "fill-white" : ""}`} />
                        </Button>
                        <Button variant="ghost" size="icon" className="rounded-full text-white hover:bg-white/20">
                            <Trash2 className="w-5 h-5" />
                        </Button>
                        <Button
                            variant="ghost"
                            size="icon"
                            onClick={() => setShowInfo(!showInfo)}
                            className={`rounded-full transition-colors ${showInfo ? "bg-white text-black hover:bg-white/90" : "text-white hover:bg-white/20"}`}
                        >
                            <Info className="w-5 h-5" />
                        </Button>
                    </div>
                </div>

                {/* The Image */}
                <div className="flex-1 flex items-center justify-center p-4">
                    <img
                        src={photo.url}
                        alt="View"
                        className="w-full h-full object-contain"
                    />
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
