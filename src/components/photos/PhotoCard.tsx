import { useState, useRef } from "react";
import type { Photo } from "@/types/photo.type";
import { cn } from "@/lib/utils";
import { useSelection } from "@/contexts/SelectionContext";
import { Heart, CheckCircle2, Circle } from "lucide-react";
import { AuraSeekApi } from "@/lib/api";

type PhotoCardProps = {
  photo: Photo;
  onClick?: () => void;
  selectionMode?: boolean;
};

export function PhotoCard({ photo, onClick, selectionMode = false }: PhotoCardProps) {
  const { selectedIds, toggleSelection } = useSelection();
  const isSelected = selectedIds.has(photo.id);
  const [isFavorite, setIsFavorite] = useState(photo.favorite ?? false);
  const imgRef = useRef<HTMLImageElement>(null);

  const handleSelect = (e: React.MouseEvent) => {
    e.stopPropagation();
    toggleSelection(photo.id);
  };

  const handleFavorite = async (e: React.MouseEvent) => {
    e.stopPropagation();
    const newState = !isFavorite;
    setIsFavorite(newState);
    // Dispatch event for optimistic update in parent
    window.dispatchEvent(new CustomEvent("photo_toggle_favorite", { detail: { id: photo.id } }));
    
    try {
      await AuraSeekApi.toggleFavorite(photo.id);
    } catch {
      setIsFavorite(!newState);
      // Re-trigger global refresh on failure
      window.dispatchEvent(new Event("refresh_photos"));
    }
  };

  return (
    <button
      type="button"
      onClick={selectionMode ? handleSelect : onClick}
      className="group relative block aspect-[4/3] overflow-hidden bg-background"
    >
      <div className={cn(
        "w-full h-full transition-all duration-200 ease-out relative",
        isSelected && selectionMode ? "p-3" : "p-0"
      )}>
        <img
          ref={imgRef}
          src={photo.url}
          alt="Photo"
          className={cn(
            "h-full w-full select-none object-cover transition-transform duration-500 ease-out",
            !(isSelected && selectionMode) && "group-hover:scale-[1.03]",
            isSelected && selectionMode && "rounded-lg"
          )}
          draggable={false}
        />

        {isSelected && selectionMode && (
          <div className="pointer-events-none absolute inset-0 bg-black/20 opacity-100 rounded-lg m-3" />
        )}
      </div>

      {selectionMode && (
        <div
          role="button"
          onClick={handleSelect}
          className={cn(
            "absolute left-2 top-2 z-10 rounded-full transition-all duration-200",
            isSelected ? "opacity-100 scale-100" : "opacity-0 group-hover:opacity-100 scale-95 hover:scale-100"
          )}
        >
          {isSelected ? (
            <CheckCircle2 className="w-6 h-6 text-primary bg-white rounded-full border-none" />
          ) : (
            <Circle className="w-6 h-6 text-white/80 hover:text-white fill-black/20" />
          )}
        </div>
      )}

      <div
        role="button"
        onClick={handleFavorite}
        className={cn(
          "absolute right-2 top-2 z-10 rounded-full p-1 transition-all duration-200",
          isFavorite
            ? "opacity-100"
            : "opacity-0 group-hover:opacity-100 hover:scale-110"
        )}
      >
        <Heart
          className={cn(
            "w-5 h-5 transition-colors drop-shadow-md",
            isFavorite ? "fill-red-500 text-red-500" : "fill-black/30 text-white/90 hover:fill-red-500 hover:text-red-500"
          )}
        />
      </div>
    </button>
  );
}
