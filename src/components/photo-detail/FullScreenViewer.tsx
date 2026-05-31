import type { Photo } from "@/types/photo.type";
import { FullScreenPhotoViewer } from "./FullScreenPhotoViewer";
import { FullScreenVideoViewer } from "./FullScreenVideoViewer";

/**
 * Router: chọn FullScreenPhotoViewer hoặc FullScreenVideoViewer theo loại media.
 * Callers giữ nguyên interface, phân biệt ảnh/video bằng file riêng.
 */
export function FullScreenViewer({
    photo,
    onClose,
    onNext,
    onPrev,
    isTrashMode = false,
    isHiddenMode = false,
    activeFaceId,
    activeClassName,
}: {
    photo: Photo;
    onClose: () => void;
    onNext?: () => void;
    onPrev?: () => void;
    isTrashMode?: boolean;
    isHiddenMode?: boolean;
    activeFaceId?: string;
    activeClassName?: string;
}) {
    if (photo.type === "video") {
        return (
            <FullScreenVideoViewer
                photo={photo}
                onClose={onClose}
                onNext={onNext}
                onPrev={onPrev}
                isTrashMode={isTrashMode}
                isHiddenMode={isHiddenMode}
            />
        );
    }
    return (
        <FullScreenPhotoViewer
            photo={photo}
            onClose={onClose}
            onNext={onNext}
            onPrev={onPrev}
            isTrashMode={isTrashMode}
            isHiddenMode={isHiddenMode}
            activeFaceId={activeFaceId}
            activeClassName={activeClassName}
        />
    );
}
