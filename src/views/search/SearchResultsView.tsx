import { useState, useMemo, useCallback } from "react";
import { ArrowLeft, Sparkles, SortAsc } from "lucide-react";
import { Button } from "@/components/ui/button";
import type { SearchResult } from "@/lib/api";
import { localFileUrl, streamFileUrlSync } from "@/lib/api";
import { FullScreenViewer } from "@/components/photo-detail/FullScreenViewer";
import { PhotoCard } from "@/components/photos/PhotoCard";
import type { Photo } from "@/types/photo.type";

interface SearchResultsViewProps {
    results: SearchResult[];
    query?: string;
    isLoading?: boolean;
    onBack: () => void;
}

export function SearchResultsView({ results, query, isLoading, onBack }: SearchResultsViewProps) {
    const [selectedPhoto, setSelectedPhoto] = useState<Photo | null>(null);

    // Map SearchResult → Photo (same shape used by PhotoCard / FullScreenViewer)
    const searchPhotos = useMemo<Photo[]>(() => {
        return results.map((result) => {
            const isVideo = result.media_type === "video";

            // Resolve thumbnail for BOTH photos and videos (same logic as home/TimelineView)
            let thumbnailUrl: string | undefined;
            if (result.thumbnail_path) {
                if (
                    result.thumbnail_path.startsWith("/") ||
                    /^[A-Za-z]:\\/.test(result.thumbnail_path)
                ) {
                    thumbnailUrl = streamFileUrlSync(result.thumbnail_path);
                } else {
                    thumbnailUrl = localFileUrl(result.thumbnail_path);
                }
            }

            return {
                id: result.media_id,
                url: localFileUrl(result.file_path),
                takenAt: result.metadata.created_at || new Date().toISOString(),
                createdAt: result.metadata.created_at || new Date().toISOString(),
                sizeBytes: 0,
                width: result.width || 0,
                height: result.height || 0,
                objects: result.metadata.objects || [],
                faces: result.metadata.faces || [],
                type: isVideo ? "video" : "photo",
                labels: result.metadata.objects || [],
                favorite: false,
                detectedObjects: result.detected_objects || [],
                detectedFaces: result.detected_faces || [],
                thumbnailUrl,
                filePath: result.file_path,
            } as Photo;
        });
    }, [results]);

    const currentPhotoIndex = useMemo(() => {
        if (!selectedPhoto) return -1;
        return searchPhotos.findIndex((p) => p.id === selectedPhoto.id);
    }, [selectedPhoto, searchPhotos]);

    const handleNextPhoto = useCallback(() => {
        if (currentPhotoIndex >= 0 && currentPhotoIndex < searchPhotos.length - 1) {
            setSelectedPhoto(searchPhotos[currentPhotoIndex + 1]);
        }
    }, [currentPhotoIndex, searchPhotos]);

    const handlePrevPhoto = useCallback(() => {
        if (currentPhotoIndex > 0) {
            setSelectedPhoto(searchPhotos[currentPhotoIndex - 1]);
        }
    }, [currentPhotoIndex, searchPhotos]);

    if (isLoading) {
        return (
            <div className="flex-1 flex flex-col items-center justify-center gap-4 text-muted-foreground">
                <div className="relative w-16 h-16">
                    <div className="absolute inset-0 rounded-full border-2 border-primary/20 animate-ping" />
                    <div className="absolute inset-2 rounded-full border-2 border-primary/40 animate-ping [animation-delay:150ms]" />
                    <Sparkles className="absolute inset-0 m-auto w-7 h-7 text-primary animate-pulse" />
                </div>
                <p className="font-medium">Đang tìm kiếm bằng AI...</p>
                <p className="text-sm opacity-60">Đang so sánh embedding vectors</p>
            </div>
        );
    }

    return (
        <div className="flex flex-col h-full w-full">
            {/* Header */}
            <div className="h-14 flex items-center px-4 shrink-0 bg-background/95 backdrop-blur z-20 border-b border-border/10 sticky top-0">
                <Button
                    variant="ghost"
                    size="icon"
                    onClick={onBack}
                    className="rounded-full mr-3 text-muted-foreground hover:text-foreground"
                >
                    <ArrowLeft className="w-5 h-5" />
                </Button>
                <div className="flex flex-col">
                    <span className="font-medium tracking-tight">
                        {query ? `Kết quả: "${query}"` : "Kết quả tìm kiếm"}
                    </span>
                    <span className="text-xs text-muted-foreground">
                        {results.length} kết quả · sắp xếp theo độ tương đồng
                    </span>
                </div>
                <div className="flex-1" />
                <div className="flex items-center gap-1 text-xs text-muted-foreground mr-2">
                    <SortAsc className="w-3.5 h-3.5" />
                    <span>Similarity ↓</span>
                </div>
            </div>

            {/* Results Grid */}
            <div className="flex-1 overflow-y-auto px-4 py-4">
                {results.length === 0 ? (
                    <div className="flex flex-col items-center justify-center h-full gap-4 text-muted-foreground">
                        <Sparkles className="w-12 h-12 opacity-20" />
                        <p className="text-lg font-medium">Không tìm thấy kết quả</p>
                        <p className="text-sm opacity-60">
                            Thử thay đổi từ khóa hoặc sử dụng ảnh tìm kiếm
                        </p>
                    </div>
                ) : (
                    <div className="grid grid-cols-2 sm:grid-cols-3 md:grid-cols-4 lg:grid-cols-5 xl:grid-cols-6 gap-2">
                        {searchPhotos.map((photo) => (
                            <PhotoCard
                                key={photo.id}
                                photo={photo}
                                className="aspect-square w-full"
                                onClick={() => setSelectedPhoto(photo)}
                                selectionMode={false}
                                showBbox={true}
                                overlayShowFaces={false}
                                overlayShowLabels={false}
                            />
                        ))}
                    </div>
                )}
            </div>

            {selectedPhoto && (
                <FullScreenViewer
                    photo={selectedPhoto}
                    onClose={() => setSelectedPhoto(null)}
                    onNext={
                        currentPhotoIndex < searchPhotos.length - 1 ? handleNextPhoto : undefined
                    }
                    onPrev={currentPhotoIndex > 0 ? handlePrevPhoto : undefined}
                />
            )}
        </div>
    );
}
