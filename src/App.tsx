import { useState, useEffect, useCallback } from "react";
import { SidebarProvider } from "@/components/ui/sidebar";
import { AppSidebar } from "./components/layout/AppSidebar";
import { AppTopbar } from "./components/layout/AppTopbar";
import { TooltipProvider } from "@/components/ui/tooltip";
import { ThemeProvider } from "./components/theme-provider";
import { SelectionProvider } from "@/contexts/SelectionContext";
import { TimelineView } from "@/views/timeline";
import { PeopleView } from "@/views/people/PeopleView";
import { DuplicatesView } from "@/views/duplicates/DuplicatesView";
import { AlbumsView } from "@/views/albums/AlbumsView";
import { FilteredGalleryView } from "@/views/gallery/FilteredGalleryView";
import { SearchResultsView } from "@/views/search/SearchResultsView";
import { AuraSeekApi, type SearchResult, type TimelineGroup, type PersonGroup, type SearchFilters as ApiFilters } from "@/lib/api";
import type { Photo } from "@/types/photo.type";

type AppRoute = {
  view: string;
  payload?: any;
};

export type ActiveFilters = {
  object?: string;
  face?: string;
  month?: number;
  year?: number;
  mediaType?: string;
};

function App() {
  const [route, setRoute] = useState<AppRoute>({ view: "timeline" });
  const [searchQuery, setSearchQuery] = useState("");
  const [searchImagePath, setSearchImagePath] = useState<string | null>(null);
  const [activeFilters, setActiveFilters] = useState<ActiveFilters>({});

  // Data state
  const [timelineGroups, setTimelineGroups] = useState<TimelineGroup[]>([]);
  const [people, setPeople] = useState<PersonGroup[]>([]);
  const [searchResults, setSearchResults] = useState<SearchResult[]>([]);
  const [isSearching, setIsSearching] = useState(false);
  const [isInitialized, setIsInitialized] = useState(false);
  const [initError, setInitError] = useState<string | null>(null);

  // Also maintain photos array for backwards compat with older components
  const [photos, setPhotos] = useState<Photo[]>([]);

  // ── Init ────────────────────────────────────────────────────────────────────

  useEffect(() => {
    const initialize = async () => {
      try {
        await AuraSeekApi.init();
        setIsInitialized(true);
        await loadTimeline();
      } catch (err: any) {
        // Graceful fallback: app still usable, DB may not be connected
        console.warn("AuraSeek init:", err);
        setInitError(String(err));
        setIsInitialized(true);
      }
    };
    initialize();
  }, []);

  const loadTimeline = useCallback(async () => {
    try {
      const groups = await AuraSeekApi.getTimeline();
      setTimelineGroups(groups);

      // Convert to legacy Photo[] format for components that still use it
      const allPhotos: Photo[] = groups.flatMap(g =>
        g.items.map(item => ({
          id: item.media_id,
          url: item.file_path,
          takenAt: item.created_at || new Date().toISOString(),
          createdAt: item.created_at || new Date().toISOString(),
          sizeBytes: 0,
          width: item.width || 0,
          height: item.height || 0,
          objects: item.objects,
          faces: item.faces,
          type: item.media_type as "photo" | "video",
          labels: item.objects,
        }))
      );
      setPhotos(allPhotos);
    } catch (err) {
      console.warn("Failed to load timeline:", err);
    }
  }, []);

  const loadPeople = useCallback(async () => {
    try {
      const p = await AuraSeekApi.getPeople();
      setPeople(p);
    } catch (err) {
      console.warn("Failed to load people:", err);
    }
  }, []);

  // ── Search ──────────────────────────────────────────────────────────────────

  const handleSearch = useCallback(async (text: string, imagePath?: string | null) => {
    if (!text.trim() && !imagePath) {
      setSearchResults([]);
      return;
    }

    setIsSearching(true);
    const filters: ApiFilters = {
      object: activeFilters.object,
      face: activeFilters.face,
      month: activeFilters.month,
      year: activeFilters.year,
      media_type: activeFilters.mediaType,
    };

    try {
      let results: SearchResult[];
      if (text && imagePath) {
        results = await AuraSeekApi.searchCombined(text, imagePath, filters);
      } else if (imagePath) {
        results = await AuraSeekApi.searchImage(imagePath, filters);
      } else {
        results = await AuraSeekApi.searchText(text, filters);
      }
      setSearchResults(results);
      if (results.length > 0) {
        setRoute({ view: "search_results" });
      }
    } catch (err) {
      console.error("Search failed:", err);
    } finally {
      setIsSearching(false);
    }
  }, [activeFilters]);

  const handleSearchSubmit = useCallback(() => {
    handleSearch(searchQuery, searchImagePath);
  }, [searchQuery, searchImagePath, handleSearch]);

  const handleFiltersChange = useCallback((filters: ActiveFilters) => {
    setActiveFilters(filters);
  }, []);

  // ── Navigation ──────────────────────────────────────────────────────────────

  const handleNavClick = useCallback((key: string) => {
    setRoute({ view: key });
    setSearchResults([]);
    setSearchQuery("");
    setSearchImagePath(null);
    if (key === "timeline") loadTimeline();
    if (key === "people") loadPeople();
  }, [loadTimeline, loadPeople]);

  // Listen for external refresh events
  useEffect(() => {
    const handler = () => loadTimeline();
    window.addEventListener("refresh_photos", handler);
    return () => window.removeEventListener("refresh_photos", handler);
  }, [loadTimeline]);

  // ── Render ──────────────────────────────────────────────────────────────────

  const renderView = () => {
    switch (route.view) {
      case "search_results":
        return (
          <SearchResultsView
            results={searchResults}
            query={searchQuery}
            isLoading={isSearching}
            onBack={() => setRoute({ view: "timeline" })}
          />
        );
      case "albums":
        return (
          <AlbumsView
            photos={photos}
            onNavigate={(payload) => setRoute({ view: "filtered", payload: { type: "album", ...payload } })}
          />
        );
      case "people":
        return (
          <PeopleView
            photos={photos}
            people={people}
            onNavigate={(payload) => setRoute({ view: "filtered", payload: { type: "person", ...payload } })}
          />
        );
      case "duplicates":
        return <DuplicatesView />;
      case "filtered":
        return (
          <FilteredGalleryView
            title={route.payload?.title || "Danh sách ảnh"}
            filterType={route.payload?.type}
            filterPayload={route.payload?.id}
            photos={photos}
            onBack={() => setRoute({ view: route.payload?.type === "album" ? "albums" : "people" })}
          />
        );
      case "favorites":
        return (
          <FilteredGalleryView
            title="Ảnh yêu thích"
            filterType="favorites"
            photos={photos}
            onBack={() => setRoute({ view: "timeline" })}
          />
        );
      case "recent":
        return (
          <FilteredGalleryView
            title="Tin mới"
            subtitle="Trong vòng 7 ngày qua"
            filterType="recent"
            photos={photos}
            onBack={() => setRoute({ view: "timeline" })}
          />
        );
      case "timeline":
      default:
        return (
          <TimelineView
            timelineGroups={timelineGroups}
            photos={photos}
            searchQuery={searchQuery}
            isLoading={!isInitialized}
          />
        );
    }
  };

  return (
    <ThemeProvider defaultTheme="system" storageKey="vite-ui-theme">
      <SelectionProvider>
        <TooltipProvider>
          <SidebarProvider>
            <AppSidebar activeKey={route.view} onNavClick={handleNavClick} />

            <main className="flex flex-col flex-1 h-screen overflow-hidden">
              <AppTopbar
                searchQuery={searchQuery}
                onSearchQueryChange={setSearchQuery}
                searchImagePath={searchImagePath}
                onSearchImageChange={setSearchImagePath}
                onSearchSubmit={handleSearchSubmit}
                isSearching={isSearching}
                onFiltersChange={handleFiltersChange}
                activeFilters={activeFilters}
                initError={initError}
              />
              {renderView()}
            </main>
          </SidebarProvider>
        </TooltipProvider>
      </SelectionProvider>
    </ThemeProvider>
  );
}

export default App;
