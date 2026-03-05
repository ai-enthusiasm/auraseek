import { SidebarTrigger } from "@/components/ui/sidebar";
import {
  Search, Filter, Moon, Sun, X, Share2, Plus, Trash2,
  History, Image as ImageIcon, Upload, AlertCircle
} from "lucide-react";
import { Button } from "@/components/ui/button";
import { useTheme } from "../theme-provider";
import { useSelection } from "@/contexts/SelectionContext";
import { FilterPanel } from "@/components/common/FilterPanel";
import { useState, useRef, useCallback } from "react";
import type { ActiveFilters } from "@/App";
import { AuraSeekApi } from "@/lib/api";

interface AppTopbarProps {
  totalImages?: number;
  searchQuery?: string;
  onSearchQueryChange?: (q: string) => void;
  searchImagePath?: string | null;
  onSearchImageChange?: (path: string | null) => void;
  onSearchSubmit?: () => void;
  isSearching?: boolean;
  onFiltersChange?: (filters: ActiveFilters) => void;
  activeFilters?: ActiveFilters;
  initError?: string | null;
}

export function AppTopbar({
  totalImages = 0,
  searchQuery = "",
  onSearchQueryChange,
  searchImagePath,
  onSearchImageChange,
  onSearchSubmit,
  isSearching = false,
  onFiltersChange,
  activeFilters,
  initError,
}: AppTopbarProps) {
  const { theme, setTheme } = useTheme();
  const { selectedIds, clearSelection } = useSelection();
  const [showFilters, setShowFilters] = useState(false);
  const [searchFocused, setSearchFocused] = useState(false);
  const [searchHistory, setSearchHistory] = useState<string[]>([]);
  const fileInputRef = useRef<HTMLInputElement>(null);

  const hasActiveFilters = activeFilters && Object.values(activeFilters).some(v => v !== undefined);

  // Load search history on focus
  const handleFocus = useCallback(async () => {
    setSearchFocused(true);
    try {
      const history = await AuraSeekApi.getSearchHistory(8);
      setSearchHistory(history.map((h: any) => h.query).filter(Boolean));
    } catch {
      setSearchHistory(["Chó chạy trên cỏ", "Biển đà nẵng", "Gia đình"]);
    }
  }, []);

  const handleKeyDown = (e: React.KeyboardEvent<HTMLInputElement>) => {
    if (e.key === "Enter") {
      onSearchSubmit?.();
      setSearchFocused(false);
    }
  };

  const handleImageUpload = (e: React.ChangeEvent<HTMLInputElement>) => {
    const file = e.target.files?.[0];
    if (!file) return;
    // For Tauri, get the path from the file object
    const path = (file as any).path || file.name;
    onSearchImageChange?.(path);
  };

  const clearSearch = () => {
    onSearchQueryChange?.("");
    onSearchImageChange?.(null);
  };

  if (selectedIds.size > 0) {
    return (
      <div className="flex items-center gap-4 px-4 h-16 shrink-0 top-0 sticky z-10 bg-primary/10 saturate-150 backdrop-blur-2xl supports-[backdrop-filter]:bg-primary/20 transition-colors">
        <Button variant="ghost" size="icon" onClick={clearSelection} className="rounded-full text-primary hover:bg-primary/20">
          <X className="w-5 h-5" />
        </Button>
        <span className="font-medium text-[1.1rem] text-primary">{selectedIds.size} đã chọn</span>
        <div className="flex-1" />
        <div className="flex items-center gap-2 text-primary">
          <Button variant="ghost" size="icon" className="rounded-full hover:bg-primary/20 text-primary"><Share2 className="w-5 h-5" /></Button>
          <Button variant="ghost" size="icon" className="rounded-full hover:bg-primary/20 text-primary"><Plus className="w-5 h-5" /></Button>
          <Button variant="ghost" size="icon" className="rounded-full hover:bg-primary/20 text-primary"><Trash2 className="w-5 h-5" /></Button>
        </div>
      </div>
    );
  }

  const imageFileName = searchImagePath
    ? searchImagePath.split(/[/\\]/).pop() || searchImagePath
    : null;

  return (
    <div className="flex items-center gap-4 px-4 h-16 shrink-0 top-0 sticky z-[40] bg-background/50 saturate-150 backdrop-blur-2xl supports-[backdrop-filter]:bg-background/20 transition-colors">

      <SidebarTrigger className="shrink-0 rounded-full w-10 h-10 ml-1 hover:bg-muted text-muted-foreground" />

      {/* Init Error Badge */}
      {initError && (
        <div className="hidden sm:flex items-center gap-1.5 px-2 py-1 rounded-full bg-amber-500/10 border border-amber-500/20 text-amber-600 dark:text-amber-400 text-xs">
          <AlertCircle className="w-3 h-3" />
          <span>DB offline</span>
        </div>
      )}

      <div className="flex-1 max-w-3xl flex items-center gap-2 mx-auto">

        {/* Image Search Chip */}
        {imageFileName && (
          <div className="flex items-center gap-1.5 pl-2 pr-1 py-1 rounded-full bg-primary/10 border border-primary/20 text-xs text-primary shrink-0 max-w-[140px]">
            <ImageIcon className="w-3 h-3 shrink-0" />
            <span className="truncate">{imageFileName}</span>
            <button
              onClick={() => onSearchImageChange?.(null)}
              className="rounded-full hover:bg-primary/20 p-0.5 ml-0.5 shrink-0"
            >
              <X className="w-3 h-3" />
            </button>
          </div>
        )}

        {/* Search Input */}
        <div className="relative flex-1 group z-50">
          <Search className={`absolute left-4 top-1/2 -translate-y-1/2 h-5 w-5 transition-colors ${searchFocused ? 'text-primary' : 'text-muted-foreground'}`} />
          <input
            type="text"
            id="search-input"
            value={searchQuery}
            onChange={(e) => onSearchQueryChange?.(e.target.value)}
            onFocus={handleFocus}
            onBlur={() => setTimeout(() => setSearchFocused(false), 200)}
            onKeyDown={handleKeyDown}
            placeholder={searchImagePath ? "Thêm mô tả (tuỳ chọn)..." : "Tìm theo văn bản, đối tượng, tháng..."}
            className={`w-full h-12 bg-muted/60 hover:bg-muted focus:bg-background border border-transparent focus:border-border/50 border-input pl-12 pr-12 text-[0.95rem] outline-none transition-all ${searchFocused ? 'rounded-t-2xl shadow-lg border-b-border/20 bg-background' : 'rounded-full shadow-sm focus:shadow-md'}`}
          />

          {/* Clear / Upload buttons inside input */}
          <div className="absolute right-3 top-1/2 -translate-y-1/2 flex items-center gap-1">
            {(searchQuery || searchImagePath) && (
              <button onClick={clearSearch} className="rounded-full p-1 hover:bg-muted text-muted-foreground">
                <X className="w-4 h-4" />
              </button>
            )}
            <button
              onClick={() => fileInputRef.current?.click()}
              className="rounded-full p-1 hover:bg-muted text-muted-foreground hover:text-primary transition-colors"
              title="Tìm kiếm bằng ảnh"
            >
              <Upload className="w-4 h-4" />
            </button>
          </div>

          {/* Hidden file input */}
          <input
            ref={fileInputRef}
            type="file"
            accept="image/*"
            className="hidden"
            onChange={handleImageUpload}
          />

          {/* Search History Dropdown */}
          {searchFocused && !searchQuery && !searchImagePath && searchHistory.length > 0 && (
            <div className="absolute top-full left-0 right-0 bg-background border border-t-0 border-border/50 rounded-b-2xl shadow-2xl overflow-hidden py-2 px-2 animate-in fade-in slide-in-from-top-2 duration-200">
              <div className="text-xs font-semibold text-muted-foreground px-3 py-2 uppercase tracking-wider">Tìm kiếm gần đây</div>
              {searchHistory.map((q, i) => (
                <button
                  key={i}
                  onClick={() => { onSearchQueryChange?.(q); onSearchSubmit?.(); }}
                  className="w-full flex items-center gap-3 px-3 py-2 hover:bg-muted/50 rounded-xl cursor-pointer text-sm text-left"
                >
                  <History className="w-4 h-4 text-muted-foreground shrink-0" />
                  <span className="flex-1 truncate">{q}</span>
                </button>
              ))}
            </div>
          )}
        </div>

        {/* Search Button */}
        <Button
          id="search-submit-btn"
          onClick={() => { onSearchSubmit?.(); setSearchFocused(false); }}
          disabled={isSearching || (!searchQuery.trim() && !searchImagePath)}
          className="h-10 px-4 rounded-full shrink-0 font-medium"
          size="sm"
        >
          {isSearching ? (
            <span className="flex items-center gap-2">
              <div className="w-4 h-4 border-2 border-white/30 border-t-white rounded-full animate-spin" />
              Đang tìm...
            </span>
          ) : (
            "Tìm kiếm"
          )}
        </Button>

        {/* Filter Button */}
        <Button
          variant="ghost"
          size="icon"
          onClick={() => setShowFilters(true)}
          className={`shrink-0 rounded-full w-10 h-10 hover:bg-muted text-muted-foreground hover:text-foreground relative ${hasActiveFilters ? 'text-primary' : ''}`}
        >
          <Filter className="w-5 h-5" />
          {hasActiveFilters && (
            <div className="absolute top-2 right-2.5 w-2 h-2 bg-primary rounded-full ring-2 ring-background" />
          )}
        </Button>
      </div>

      <div className="flex items-center gap-3 shrink-0 mr-2 text-sm text-muted-foreground">
        <span className="hidden sm:inline-block font-medium">{totalImages > 0 ? `${totalImages} Ảnh & Video` : ""}</span>
        <Button
          variant="ghost"
          size="icon"
          className="rounded-full w-10 h-10 ml-2"
          onClick={() => setTheme(theme === 'dark' ? 'light' : 'dark')}
        >
          {theme === 'dark' ? <Sun className="w-5 h-5" /> : <Moon className="w-5 h-5" />}
        </Button>
      </div>

      <FilterPanel
        open={showFilters}
        onOpenChange={setShowFilters}
        activeFilters={activeFilters}
        onFiltersChange={onFiltersChange}
      />
    </div>
  );
}
