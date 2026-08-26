import {
  useState,
  useEffect,
  useRef,
  useCallback,
  useMemo,
  type CSSProperties,
  type ReactElement,
} from "react";
import { Grid } from "react-window";
import { useInfiniteLoader } from "react-window-infinite-loader";
import type { Photo } from "@/types/photo.type";
import type { TimelinePageItem } from "@/lib/api";
import { AuraSeekApi, localFileUrl, streamFileUrlSync } from "@/lib/api";
import { PhotoCard } from "./PhotoCard";

// ─── Config ──────────────────────────────────────────────────────────────────
const PAGE_SIZE = 200;
const ROW_HEIGHT = 200; // px — aspect ratio ~4:3 grid cells
const GAP = 6; // px between cells

// ─── Types ───────────────────────────────────────────────────────────────────
interface VirtualPhotoGridProps {
  /** Pre-loaded photos for non-paginated mode (search results, album, etc.) */
  photos?: Photo[];
  /** If true, use paginated API from backend instead of pre-loaded photos */
  paginated?: boolean;
  onPhotoClick?: (photo: Photo) => void;
  selectionMode?: boolean;
  showBbox?: boolean;
  mediaType?: "video" | "photo";
  activeFaceId?: string;
  activeClassName?: string;
}

// ─── Helpers ─────────────────────────────────────────────────────────────────
function pageItemToPhoto(item: TimelinePageItem): Photo {
  const isVideo = item.media_type === "video";
  const url = localFileUrl(item.file_path);

  let thumbnailUrl: string | undefined;
  if (item.thumbnail_path) {
    if (
      item.thumbnail_path.startsWith("/") ||
      /^[A-Za-z]:\\/.test(item.thumbnail_path)
    ) {
      thumbnailUrl = streamFileUrlSync(item.thumbnail_path);
    } else {
      thumbnailUrl = localFileUrl(item.thumbnail_path);
    }
  }

  return {
    id: item.media_id,
    url,
    takenAt: item.created_at || new Date().toISOString(),
    createdAt: item.created_at || new Date().toISOString(),
    sizeBytes: 0,
    width: item.width || 0,
    height: item.height || 0,
    objects: [],
    faces: [],
    type: isVideo ? "video" : "photo",
    labels: [],
    favorite: item.favorite,
    thumbnailUrl,
    filePath: item.file_path,
  } as Photo;
}

type GridCellProps = {
  photos: Photo[];
  columnCount: number;
  itemCount: number;
  cellWidth: number;
  cellHeight: number;
  onPhotoClick?: (photo: Photo) => void;
  selectionMode: boolean;
  showBbox: boolean;
  activeFaceId?: string;
  activeClassName?: string;
};

// ─── Cell Component ──────────────────────────────────────────────────────────
function PhotoCell({
  columnIndex,
  rowIndex,
  style,
  photos,
  columnCount,
  itemCount,
  cellWidth,
  cellHeight,
  onPhotoClick,
  selectionMode,
  showBbox,
  activeFaceId,
  activeClassName,
}: {
  columnIndex: number;
  rowIndex: number;
  style: CSSProperties;
} & GridCellProps): ReactElement | null {
  const photoIndex = rowIndex * columnCount + columnIndex;
  if (photoIndex >= itemCount) return null;

  const photo = photos[photoIndex];

  // Apply gap to style
  const adjustedStyle: CSSProperties = {
    ...style,
    left: (style.left as number) + columnIndex * GAP,
    top: (style.top as number) + rowIndex * GAP,
    width: cellWidth,
    height: cellHeight,
  };

  if (!photo) {
    // Loading placeholder
    return (
      <div style={adjustedStyle}>
        <div className="w-full h-full bg-zinc-200 dark:bg-zinc-800 rounded-2xl animate-pulse" />
      </div>
    );
  }

  return (
    <div style={adjustedStyle}>
      <PhotoCard
        photo={photo}
        className="w-full h-full"
        onClick={onPhotoClick ? () => onPhotoClick(photo) : undefined}
        selectionMode={selectionMode}
        showBbox={showBbox}
        activeFaceId={activeFaceId}
        activeClassName={activeClassName}
      />
    </div>
  );
}

// ─── Component ───────────────────────────────────────────────────────────────
export function VirtualPhotoGrid({
  photos: externalPhotos,
  paginated = false,
  onPhotoClick,
  selectionMode = false,
  showBbox = false,
  mediaType,
  activeFaceId,
  activeClassName,
}: VirtualPhotoGridProps) {
  // Container ref for measuring available width/height
  const containerRef = useRef<HTMLDivElement>(null);
  const [containerWidth, setContainerWidth] = useState(800);
  const [containerHeight, setContainerHeight] = useState(600);

  // Paginated state
  const [paginatedPhotos, setPaginatedPhotos] = useState<Photo[]>([]);
  const [totalItems, setTotalItems] = useState(0);
  const [loadedPages, setLoadedPages] = useState<Set<number>>(new Set());
  const [isLoading, setIsLoading] = useState(false);

  // Which photos to actually render
  const allPhotos = useMemo(() => {
    if (!paginated && externalPhotos) {
      // Filter by media type if needed
      if (mediaType === "video") return externalPhotos.filter((p) => p.type === "video");
      if (mediaType === "photo") return externalPhotos.filter((p) => p.type !== "video");
      return externalPhotos;
    }
    return paginatedPhotos;
  }, [paginated, externalPhotos, paginatedPhotos, mediaType]);

  // Compute column count based on container width
  const columnCount = useMemo(() => {
    if (containerWidth < 400) return 2;
    if (containerWidth < 600) return 3;
    if (containerWidth < 800) return 4;
    if (containerWidth < 1100) return 5;
    if (containerWidth < 1400) return 6;
    return 7;
  }, [containerWidth]);

  const cellWidth = (containerWidth - GAP * (columnCount - 1)) / columnCount;
  const cellHeight = ROW_HEIGHT;

  const itemCount = paginated ? totalItems : allPhotos.length;
  const rowCount = Math.ceil(itemCount / columnCount);

  // ── Measure container ──────────────────────────────────────────────────
  useEffect(() => {
    const el = containerRef.current;
    if (!el) return;

    const ro = new ResizeObserver((entries) => {
      for (const entry of entries) {
        setContainerWidth(entry.contentRect.width);
        setContainerHeight(entry.contentRect.height);
      }
    });
    ro.observe(el);
    return () => ro.disconnect();
  }, []);

  // ── Initial page load (paginated mode) ─────────────────────────────────
  useEffect(() => {
    if (!paginated) return;
    loadPage(0);
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [paginated, mediaType]);

  const loadPage = useCallback(
    async (pageIndex: number) => {
      if (loadedPages.has(pageIndex)) return;

      setIsLoading(true);
      try {
        const offset = pageIndex * PAGE_SIZE;
        const resp = await AuraSeekApi.getTimelinePage(offset, PAGE_SIZE);
        setTotalItems(resp.total);

        const newPhotos = resp.items
          .map(pageItemToPhoto)
          .filter((p) => {
            if (mediaType === "video") return p.type === "video";
            if (mediaType === "photo") return p.type !== "video";
            return true;
          });

        setPaginatedPhotos((prev) => {
          const updated = [...prev];
          // Place items at the correct indices
          for (let i = 0; i < newPhotos.length; i++) {
            updated[offset + i] = newPhotos[i];
          }
          return updated;
        });

        setLoadedPages((prev) => new Set(prev).add(pageIndex));
      } catch (err) {
        console.warn("[VirtualPhotoGrid] Page load failed:", err);
      } finally {
        setIsLoading(false);
      }
    },
    [loadedPages, mediaType]
  );

  // ── InfiniteLoader hook ────────────────────────────────────────────────
  const onRowsRendered = useInfiniteLoader({
    isRowLoaded: (index: number) => {
      if (!paginated) return index < allPhotos.length;
      // Check if any photo in this row is loaded
      const startIdx = index * columnCount;
      return !!allPhotos[startIdx];
    },
    loadMoreRows: async (startIndex: number, _stopIndex: number) => {
      if (!paginated) return;
      const photoIndex = startIndex * columnCount;
      const pageIndex = Math.floor(photoIndex / PAGE_SIZE);
      await loadPage(pageIndex);
    },
    rowCount: rowCount,
    threshold: 5,
  });

  // Cell data to pass down
  const cellData = useMemo<GridCellProps>(
    () => ({
      photos: allPhotos,
      columnCount,
      itemCount,
      cellWidth,
      cellHeight,
      onPhotoClick,
      selectionMode,
      showBbox,
      activeFaceId,
      activeClassName,
    }),
    [allPhotos, columnCount, itemCount, cellWidth, cellHeight, onPhotoClick, selectionMode, showBbox, activeFaceId, activeClassName]
  );

  // ── Empty state ────────────────────────────────────────────────────────
  if (itemCount === 0 && !isLoading) {
    return (
      <div
        ref={containerRef}
        className="flex flex-col items-center justify-center h-64 gap-4 text-muted-foreground opacity-60"
      >
        <div className="text-5xl">{mediaType === "video" ? "🎬" : "📷"}</div>
        <div className="text-center">
          <p className="font-medium text-lg">
            {mediaType === "video" ? "Chưa có video nào" : "Chưa có ảnh nào"}
          </p>
        </div>
      </div>
    );
  }

  return (
    <div ref={containerRef} className="w-full h-full">
      {containerWidth > 0 && containerHeight > 0 && (
        <Grid<GridCellProps>
          columnCount={columnCount}
          columnWidth={cellWidth + GAP}
          rowCount={rowCount}
          rowHeight={cellHeight + GAP}
          defaultHeight={containerHeight}
          defaultWidth={containerWidth}
          cellComponent={PhotoCell}
          cellProps={cellData}
          onCellsRendered={({
            columnStartIndex: _columnStartIndex,
            columnStopIndex: _columnStopIndex,
            rowStartIndex,
            rowStopIndex,
          }) => {
            onRowsRendered({
              startIndex: rowStartIndex,
              stopIndex: rowStopIndex,
            });
          }}
          overscanCount={3}
          className="scrollbar-thin scrollbar-thumb-zinc-300 dark:scrollbar-thumb-zinc-700"
        />
      )}
    </div>
  );
}
