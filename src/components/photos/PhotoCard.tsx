import type { Photo } from "@/types/photo.type";
import { cn } from "@/lib/utils";
import { useSelection } from "@/contexts/SelectionContext";
import { CheckCircle2, Circle } from "lucide-react";

type PhotoCardProps = {
  photo: Photo;
  onClick?: () => void;
};

export function PhotoCard({ photo, onClick }: PhotoCardProps) {
  const { selectedIds, toggleSelection } = useSelection();
  const isSelected = selectedIds.has(photo.id);

  const handleSelect = (e: React.MouseEvent) => {
    e.stopPropagation();
    toggleSelection(photo.id);
  };

  return (
    <button
      type="button"
      onClick={onClick}
      className="group relative block aspect-[4/3] overflow-hidden bg-background"
    >
      <div className={cn(
        "w-full h-full transition-all duration-200 ease-out relative",
        isSelected ? "p-3" : "p-0"
      )}>
        <img
          src={photo.url}
          alt={photo.labels?.[0] ?? "Photo"}
          className={cn(
            "h-full w-full select-none object-cover transition-transform duration-500 ease-out",
            !isSelected && "group-hover:scale-[1.03]",
            isSelected && "rounded-lg"
          )}
          draggable={false}
        />
        <div className={cn(
          "pointer-events-none absolute inset-0 transition-opacity duration-300",
          isSelected ? "bg-black/20 opacity-100 rounded-lg p-3 m-3" : "bg-black/40 opacity-0 group-hover:opacity-100"
        )} />
      </div>

      {/* Checkmark Button */}
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

      {photo.favorite && (
        <div className="pointer-events-none absolute right-2 top-2 rounded-full bg-black/40 px-2 py-0.5 text-xs font-medium text-white shadow-md backdrop-blur z-10">
          ❤
        </div>
      )}
    </button>
  );
}
