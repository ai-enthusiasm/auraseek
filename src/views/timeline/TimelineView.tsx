import { useMemo, useState, useRef, useEffect, useCallback } from "react";
import type { Photo } from "@/types/photo.type";
import type { TimelineGroup, TimelinePageItem } from "@/lib/api";
import { VirtualPhotoGrid } from "@/components/photos/VirtualPhotoGrid";
import { PhotoGrid } from "@/components/photos/PhotoGrid";
import { FullScreenViewer } from "@/components/photo-detail/FullScreenViewer";
import { AuraSeekApi, localFileUrl, streamFileUrlSync } from "@/lib/api";

// ─── Config ──────────────────────────────────────────────────────────────────
const PAGE_SIZE = 200;

interface TimelineViewProps {
  timelineGroups?: TimelineGroup[];
  photos?: Photo[];
  searchQuery?: string;
  isLoading?: boolean;
  selectionMode?: boolean;
  mediaType?: "video" | "photo";
}


export function TimelineView({
  timelineGroups = [],
  photos = [],
  searchQuery = "",
  isLoading = false,
  selectionMode = false,
  mediaType,
}: TimelineViewProps) {
  const [selectedPhoto, setSelectedPhoto] = useState<Photo | null>(null);

  // ── Paginated state ─────────────────────────────────────────────────────
  const [paginatedItems, setPaginatedItems] = useState<Photo[]>([]);
  const [totalItems, setTotalItems] = useState(0);
  const [loadedPages, setLoadedPages] = useState<Set<number>>(new Set());
  const [isPaginatedLoading, setIsPaginatedLoading] = useState(false);
  const scrollContainerRef = useRef<HTMLDivElement>(null);

  // Decide if we use paginated mode:
  // - Use paginated when we have no search query and no timeline groups preloaded
  //   OR when the total is large enough to benefit from virtualization
  const usePaginated = !searchQuery.trim() && timelineGroups.length === 0 && photos.length === 0;

  // ── Page loading ────────────────────────────────────────────────────────
  const loadPage = useCallback(
    async (pageIndex: number) => {
      if (loadedPages.has(pageIndex)) return;

      setIsPaginatedLoading(true);
      try {
        const offset = pageIndex * PAGE_SIZE;
        const resp = await AuraSeekApi.getTimelinePage(offset, PAGE_SIZE);
        setTotalItems(resp.total);

        const newPhotos = resp.items.map(pageItemToPhoto);

        setPaginatedItems((prev) => {
          const updated = [...prev];
          for (let i = 0; i < newPhotos.length; i++) {
            updated[offset + i] = newPhotos[i];
          }
          return updated;
        });

        setLoadedPages((prev) => new Set(prev).add(pageIndex));
      } catch (err) {
        console.warn("[TimelineView] Page load failed:", err);
      } finally {
        setIsPaginatedLoading(false);
      }
    },
    [loadedPages]
  );

  // Load first page in paginated mode
  useEffect(() => {
    if (usePaginated && loadedPages.size === 0) {
      loadPage(0);
    }
  }, [usePaginated, loadedPages.size, loadPage]);

  // Background refresh handler for paginated mode
  const refreshLoadedPages = useCallback(async () => {
    if (!usePaginated) return;
    try {
      const pagesToRefresh = Array.from(loadedPages);
      if (pagesToRefresh.length === 0) {
        await loadPage(0);
        return;
      }

      setIsPaginatedLoading(true);
      const promises = pagesToRefresh.map(async (pageIndex) => {
        const offset = pageIndex * PAGE_SIZE;
        const resp = await AuraSeekApi.getTimelinePage(offset, PAGE_SIZE);
        return { pageIndex, offset, resp };
      });

      const results = await Promise.all(promises);
      
      if (results.length > 0) {
        const latestTotal = Math.max(...results.map(r => r.resp.total));
        setTotalItems(latestTotal);
      }

      setPaginatedItems((prev) => {
        const updated = [...prev];
        for (const { offset, resp } of results) {
          const newPhotos = resp.items.map(pageItemToPhoto);
          for (let i = 0; i < newPhotos.length; i++) {
            updated[offset + i] = newPhotos[i];
          }
        }
        return updated;
      });
    } catch (err) {
      console.warn("[TimelineView] Background refresh failed:", err);
    } finally {
      setIsPaginatedLoading(false);
    }
  }, [usePaginated, loadedPages, loadPage]);

  useEffect(() => {
    const handleRefresh = () => {
      console.log("[TimelineView] 🔄 Refreshing photos...");
      refreshLoadedPages();
    };
    window.addEventListener("refresh_photos", handleRefresh);
    return () => {
      window.removeEventListener("refresh_photos", handleRefresh);
    };
  }, [refreshLoadedPages]);

  // For paginated mode — collect all loaded photos for the virtual grid
  const paginatedFilteredPhotos = useMemo(() => {
    if (!usePaginated) return [];
    if (mediaType === "video") return paginatedItems.filter(p => p?.type === "video");
    if (mediaType === "photo") return paginatedItems.filter(p => p && p.type !== "video");
    return paginatedItems;
  }, [usePaginated, paginatedItems, mediaType]);

  // Process into sections (traditional mode)
  const sections = useMemo(() => {
    // Prefer structured timeline groups
    if (timelineGroups.length > 0) {
      return timelineGroups.map(g => ({
        id: `${g.year}-${g.month}`,
        label: g.label,
        photos: g.items
          .filter(item => {
            if (mediaType === "video") {
              return item.media_type === "video";
            }
            if (mediaType === "photo") {
              // Treat anything that is not an explicit video as "photo-like"
              return item.media_type !== "video";
            }

            if (!searchQuery.trim()) return true;
            const q = searchQuery.toLowerCase();
            return (
              item.objects.some(o => o.toLowerCase().includes(q)) ||
              item.faces.some(f => f.toLowerCase().includes(q)) ||
              item.file_path.toLowerCase().includes(q)
            );
          })
          .map(item => {
            const isVideo = item.media_type === "video";
            const isMock = item.media_id.startsWith("mock-");
            
            const url = isMock ? item.file_path : localFileUrl(item.file_path);
            
            // Use streamFileUrlSync for absolute paths (cached thumbs in data dir), else fallback cleanly.
            let thumbnailUrl = undefined;
            if (item.thumbnail_path) {
              if (isMock) {
                thumbnailUrl = item.thumbnail_path;
              } else if (item.thumbnail_path.startsWith("/") || item.thumbnail_path.match(/^[A-Za-z]:\\/)) {
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
              objects: item.objects,
              faces: item.faces,
              type: isVideo ? "video" as const : "photo" as const,
              labels: item.objects,
              favorite: item.favorite,
              detectedObjects: item.detected_objects,
              detectedFaces: item.detected_faces,
              thumbnailUrl,
              filePath: item.file_path,
            } as Photo;
          }),
      })).filter(s => s.photos.length > 0);
    }

    // Fallback: group flat photos by month
    const map = new Map<string, { id: string; label: string; photos: Photo[] }>();
    const targetPhotos = usePaginated ? paginatedFilteredPhotos : photos;
    const filteredPhotos = targetPhotos.filter(p => {
      if (!p) return false;
      if (mediaType === "video") {
        return p.type === "video";
      }
      if (mediaType === "photo") {
        // Default everything that is not marked as video into the photo bucket
        return p.type !== "video";
      }
      if (!searchQuery.trim()) return true;

      const q = searchQuery.toLowerCase();
      return (
        p.labels?.some(l => l.toLowerCase().includes(q)) ||
        p.objects?.some(o => o.toLowerCase().includes(q)) ||
        p.id.toLowerCase().includes(q)
      );
    });

    for (const photo of filteredPhotos) {
      const date = new Date(photo.takenAt);
      const id = `${date.getFullYear()}-${date.getMonth() + 1}-${date.getDate()}`;
      
      const weekday = new Intl.DateTimeFormat("vi-VN", { weekday: "long" }).format(date);
      const day = new Intl.DateTimeFormat("vi-VN", { day: "2-digit" }).format(date);
      const month = new Intl.DateTimeFormat("vi-VN", { month: "2-digit" }).format(date);
      const year = new Intl.DateTimeFormat("vi-VN", { year: "numeric" }).format(date);
      
      const label = `${weekday}, ngày ${day} tháng ${month} năm ${year}`;
      
      const existing = map.get(id);
      if (!existing) {
        map.set(id, { id, label, photos: [photo] });
      } else {
        existing.photos.push(photo);
      }
    }
    return Array.from(map.values()).sort((a, b) => a.id < b.id ? 1 : -1);
  }, [timelineGroups, photos, searchQuery, mediaType, usePaginated, paginatedFilteredPhotos]);

  // For small collections or grouped timeline, use sections mode
  // For large paginated collections, use virtual grid
  const useVirtualGrid = false; // Disable flat virtual grid to preserve monthly headers as requested

  // Flatten sections for virtual grid mode
  const allSectionPhotos = useMemo(() => {
    if (!useVirtualGrid || usePaginated) return [];
    return sections.flatMap(s => s.photos);
  }, [useVirtualGrid, usePaginated, sections]);

  // Infinite scroll handler for paginated mode
  useEffect(() => {
    if (!usePaginated) return;
    const container = scrollContainerRef.current;
    if (!container) return;

    const handleScroll = () => {
      const { scrollTop, scrollHeight, clientHeight } = container;
      // Load next page when 80% scrolled
      if (scrollTop + clientHeight > scrollHeight * 0.8) {
        const nextPage = loadedPages.size;
        if (nextPage * PAGE_SIZE < totalItems) {
          loadPage(nextPage);
        }
      }
    };

    container.addEventListener("scroll", handleScroll, { passive: true });
    return () => container.removeEventListener("scroll", handleScroll);
  }, [usePaginated, loadedPages.size, totalItems, loadPage]);

  // Memoized flat list of all visible photos for detail navigation
  const flatPhotos = useMemo(() => {
    const raw = usePaginated
      ? paginatedFilteredPhotos
      : useVirtualGrid
      ? allSectionPhotos
      : sections.flatMap((s) => s.photos);
    return raw.filter((p): p is Photo => !!p);
  }, [usePaginated, paginatedFilteredPhotos, useVirtualGrid, allSectionPhotos, sections]);

  const handleNextPhoto = useCallback(() => {
    if (!selectedPhoto) return;
    const currentIndex = flatPhotos.findIndex((p) => p.id === selectedPhoto.id);
    if (currentIndex >= 0 && currentIndex < flatPhotos.length - 1) {
      setSelectedPhoto(flatPhotos[currentIndex + 1]);
    }
  }, [selectedPhoto, flatPhotos]);

  const handlePrevPhoto = useCallback(() => {
    if (!selectedPhoto) return;
    const currentIndex = flatPhotos.findIndex((p) => p.id === selectedPhoto.id);
    if (currentIndex > 0) {
      setSelectedPhoto(flatPhotos[currentIndex - 1]);
    }
  }, [selectedPhoto, flatPhotos]);

  const currentPhotoIndex = useMemo(() => {
    if (!selectedPhoto) return -1;
    return flatPhotos.findIndex((p) => p.id === selectedPhoto.id);
  }, [selectedPhoto, flatPhotos]);

  return (
    <div className="flex relative h-full flex-1 flex-col overflow-hidden">
      <div
        ref={scrollContainerRef}
        id="timeline-scroll-container"
        className="flex-1 overflow-y-auto px-4 pb-6 pt-3 sm:px-6 lg:px-8 relative bg-background"
      >
        {/* Loading skeleton */}
        {isLoading && (
          <div className="space-y-6">
            {[1, 2, 3].map(i => (
              <div key={i} className="space-y-3 animate-pulse">
                <div className="h-6 w-32 bg-muted rounded-full" />
                <div className="grid grid-cols-4 gap-2">
                  {[1, 2, 3, 4].map(j => (
                    <div key={j} className="aspect-square bg-muted rounded-xl" />
                  ))}
                </div>
              </div>
            ))}
          </div>
        )}

        {/* Empty state */}
        {!isLoading && !usePaginated && sections.length === 0 && (
          <div className="flex flex-col items-center justify-center h-64 gap-4 text-muted-foreground opacity-60">
            <div className="text-5xl">{mediaType === "video" ? "🎬" : "📷"}</div>
            <div className="text-center">
              <p className="font-medium text-lg">
                {mediaType === "video" ? "Chưa có video nào" : "Chưa có ảnh nào"}
              </p>
              <p className="text-sm mt-1">
                Vào Cài đặt → Khởi chạy bộ quét AI để import {mediaType === "video" ? "video" : "ảnh"}
              </p>
            </div>
          </div>
        )}

        {/* Virtual Grid mode — for large collections */}
        {useVirtualGrid && !isLoading && (
          <div className="h-full">
            <VirtualPhotoGrid
              photos={usePaginated ? paginatedFilteredPhotos : allSectionPhotos}
              onPhotoClick={(photo) => setSelectedPhoto(photo)}
              selectionMode={selectionMode}
              showBbox={false}
              mediaType={mediaType}
            />
          </div>
        )}

        {/* Traditional grouped mode — for smaller collections */}
        {!useVirtualGrid && !isLoading && (
          <div className="space-y-6 sm:space-y-8 pr-6">
            {sections.map((section) => (
              <section key={section.id} id={`section-${section.id}`} className="space-y-6 pt-4">
                <div className="flex items-center justify-between mb-4 px-1">
                  <div className="font-['Montserrat'] font-semibold text-[17px] text-zinc-600 dark:text-zinc-400 tracking-wide">
                    {section.label}
                  </div>
                </div>

                <PhotoGrid
                  photos={section.photos}
                  onPhotoClick={(photo) => setSelectedPhoto(photo)}
                  selectionMode={selectionMode}
                  showBbox={false}
                />
              </section>
            ))}
          </div>
        )}

        {/* Paginated loading indicator */}
        {isPaginatedLoading && (
          <div className="flex justify-center py-8">
            <div className="w-6 h-6 border-2 border-primary border-t-transparent rounded-full animate-spin" />
          </div>
        )}
      </div>

      {/* Right side timeline scrubber */}
      {!isLoading && !useVirtualGrid && sections.length > 0 && (
        <div className="absolute right-0 top-32 bottom-32 w-8 sm:w-16 flex flex-col justify-between items-end pr-2 py-4 z-20 opacity-0 hover:opacity-100 transition-opacity duration-300">
          {(() => {
            const seenYears = new Set();
            return sections.map((sec) => {
              const [year, month] = sec.id.split('-');
              const isFirstOfYear = !seenYears.has(year);
              if (isFirstOfYear) seenYears.add(year);

              return (
                <div
                  key={sec.id}
                  className="relative cursor-pointer flex items-center justify-end w-full group/item py-0.5"
                  onClick={() => {
                    const el = document.getElementById(`section-${sec.id}`);
                    const container = document.getElementById('timeline-scroll-container');
                    if (el && container) {
                      const topPos = el.offsetTop - container.offsetTop;
                      container.scrollTo({ top: topPos, behavior: 'smooth' });
                    }
                  }}
                >
                  <div className={`transition-all w-full flex justify-end items-center`}>
                    <div className={`text-[9px] sm:text-[10px] font-bold text-muted-foreground/70 group-hover/item:hidden ${isFirstOfYear ? 'block' : 'hidden'}`}>
                      {year}
                    </div>
                    <div className={`h-1 w-1 sm:h-1.5 sm:w-1.5 rounded-full bg-muted-foreground/30 group-hover/item:hidden ${isFirstOfYear ? 'hidden' : 'block mr-1'}`}></div>

                    <div className="hidden group-hover/item:block text-[10px] sm:text-[11px] font-bold text-primary whitespace-nowrap">
                      thg {month} {year}
                    </div>
                  </div>
                </div>
              );
            });
          })()}
        </div>
      )}

      {selectedPhoto && (
        <FullScreenViewer
          photo={selectedPhoto}
          onClose={() => setSelectedPhoto(null)}
          onNext={currentPhotoIndex < flatPhotos.length - 1 ? handleNextPhoto : undefined}
          onPrev={currentPhotoIndex > 0 ? handlePrevPhoto : undefined}
        />
      )}
    </div>
  );
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
