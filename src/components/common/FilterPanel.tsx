import {
    Sheet,
    SheetContent,
    SheetDescription,
    SheetHeader,
    SheetTitle,
} from "@/components/ui/sheet";
import { Button } from "@/components/ui/button";
import { Filter, Calendar, Users, FileType, Tag, X } from "lucide-react";
import { useState, useEffect } from "react";
import type { ActiveFilters } from "@/App";

// COCO 80 class names (most common ones for UI)
const COMMON_OBJECTS = [
    "person", "dog", "cat", "car", "bicycle", "motorcycle",
    "airplane", "bus", "train", "truck", "boat", "bird",
    "horse", "cow", "elephant", "bear", "chair", "couch",
    "laptop", "phone", "book", "bottle", "cup", "pizza",
    "cake", "backpack", "umbrella", "sports ball", "kite",
];

const MONTHS = [
    { label: "Tháng 1", value: 1 }, { label: "Tháng 2", value: 2 },
    { label: "Tháng 3", value: 3 }, { label: "Tháng 4", value: 4 },
    { label: "Tháng 5", value: 5 }, { label: "Tháng 6", value: 6 },
    { label: "Tháng 7", value: 7 }, { label: "Tháng 8", value: 8 },
    { label: "Tháng 9", value: 9 }, { label: "Tháng 10", value: 10 },
    { label: "Tháng 11", value: 11 }, { label: "Tháng 12", value: 12 },
];

interface FilterPanelProps {
    open: boolean;
    onOpenChange: (open: boolean) => void;
    activeFilters?: ActiveFilters;
    onFiltersChange?: (filters: ActiveFilters) => void;
}

export function FilterPanel({ open, onOpenChange, activeFilters, onFiltersChange }: FilterPanelProps) {
    const [localFilters, setLocalFilters] = useState<ActiveFilters>(activeFilters || {});
    const [objectSearch, setObjectSearch] = useState("");

    useEffect(() => {
        if (open) {
            setLocalFilters(activeFilters || {});
        }
    }, [open, activeFilters]);

    const update = (patch: Partial<ActiveFilters>) => {
        setLocalFilters(prev => ({ ...prev, ...patch }));
    };

    const handleApply = () => {
        onFiltersChange?.(localFilters);
        onOpenChange(false);
    };

    const handleReset = () => {
        const empty: ActiveFilters = {};
        setLocalFilters(empty);
        onFiltersChange?.(empty);
        onOpenChange(false);
    };

    const activeCount = Object.values(localFilters).filter(v => v !== undefined).length;

    const filteredObjects = objectSearch
        ? COMMON_OBJECTS.filter(o => o.includes(objectSearch.toLowerCase()))
        : COMMON_OBJECTS;

    return (
        <Sheet open={open} onOpenChange={onOpenChange}>
            <SheetContent side="right" className="w-full sm:max-w-md p-0 flex flex-col bg-background border-l-border/30 shadow-2xl">
                <SheetHeader className="px-6 py-4 border-b border-border/10 bg-muted/20">
                    <SheetTitle className="flex items-center gap-2">
                        <Filter className="w-5 h-5 text-primary" />
                        Bộ lọc nâng cao
                        {activeCount > 0 && (
                            <span className="ml-auto flex items-center gap-1.5 text-xs bg-primary/10 text-primary px-2 py-0.5 rounded-full">
                                {activeCount} đang bật
                            </span>
                        )}
                    </SheetTitle>
                    <SheetDescription>
                        Kết hợp nhiều điều kiện để tìm chính xác nội dung bạn cần.
                    </SheetDescription>
                </SheetHeader>

                <div className="flex-1 overflow-y-auto px-6 py-6 space-y-8">

                    {/* Media Type */}
                    <div className="space-y-3">
                        <div className="flex items-center gap-2 font-medium text-sm">
                            <FileType className="w-4 h-4 text-muted-foreground" />
                            Loại tệp
                        </div>
                        <div className="flex flex-wrap gap-2">
                            {[
                                { label: "Tất cả", value: undefined },
                                { label: "Chỉ Ảnh", value: "image" },
                                { label: "Chỉ Video", value: "video" },
                            ].map(opt => (
                                <Button
                                    key={opt.label}
                                    variant="outline"
                                    size="sm"
                                    className={`rounded-full transition-all ${localFilters.mediaType === opt.value
                                            ? "bg-primary/10 border-primary/30 text-primary"
                                            : ""
                                        }`}
                                    onClick={() => update({ mediaType: opt.value })}
                                >
                                    {opt.label}
                                </Button>
                            ))}
                        </div>
                    </div>

                    {/* Time Filter */}
                    <div className="space-y-3">
                        <div className="flex items-center gap-2 font-medium text-sm">
                            <Calendar className="w-4 h-4 text-muted-foreground" />
                            Tháng / Năm
                        </div>

                        {/* Year */}
                        <div className="space-y-1.5">
                            <label className="text-xs text-muted-foreground">Năm</label>
                            <input
                                type="number"
                                min="2000"
                                max="2030"
                                value={localFilters.year || ""}
                                onChange={(e) => update({ year: e.target.value ? parseInt(e.target.value) : undefined })}
                                placeholder="e.g. 2024"
                                className="flex h-9 w-32 rounded-md border border-input bg-transparent px-3 py-1 text-sm shadow-sm"
                            />
                        </div>

                        {/* Month chips */}
                        <div className="flex flex-wrap gap-2">
                            {MONTHS.map(m => (
                                <Button
                                    key={m.value}
                                    variant="outline"
                                    size="sm"
                                    className={`rounded-full text-xs h-7 px-3 transition-all ${localFilters.month === m.value
                                            ? "bg-primary/10 border-primary/30 text-primary"
                                            : ""
                                        }`}
                                    onClick={() => update({ month: localFilters.month === m.value ? undefined : m.value })}
                                >
                                    {m.label}
                                </Button>
                            ))}
                        </div>
                    </div>

                    {/* Object Filter */}
                    <div className="space-y-3">
                        <div className="flex items-center gap-2 font-medium text-sm">
                            <Tag className="w-4 h-4 text-muted-foreground" />
                            Đối tượng AI (COCO)
                        </div>

                        {localFilters.object && (
                            <div className="flex items-center gap-2">
                                <span className="bg-primary/10 text-primary text-xs px-2.5 py-1 rounded-full border border-primary/20">
                                    {localFilters.object}
                                </span>
                                <button onClick={() => update({ object: undefined })} className="text-muted-foreground hover:text-foreground">
                                    <X className="w-3.5 h-3.5" />
                                </button>
                            </div>
                        )}

                        <input
                            type="text"
                            placeholder="Tìm đối tượng (dog, car, person...)"
                            value={objectSearch}
                            onChange={e => setObjectSearch(e.target.value)}
                            className="flex h-8 w-full rounded-md border border-input bg-transparent px-3 py-1 text-sm"
                        />

                        <div className="flex flex-wrap gap-1.5 max-h-32 overflow-y-auto">
                            {filteredObjects.map(obj => (
                                <button
                                    key={obj}
                                    onClick={() => update({ object: localFilters.object === obj ? undefined : obj })}
                                    className={`text-xs px-2 py-1 rounded-md border transition-all ${localFilters.object === obj
                                            ? "bg-primary/10 border-primary/30 text-primary font-medium"
                                            : "border-border/40 hover:bg-muted"
                                        }`}
                                >
                                    {obj}
                                </button>
                            ))}
                        </div>
                    </div>

                    {/* Face / Person Filter */}
                    <div className="space-y-3">
                        <div className="flex items-center gap-2 font-medium text-sm">
                            <Users className="w-4 h-4 text-muted-foreground" />
                            Người trong ảnh
                        </div>
                        <div className="flex gap-2">
                            <input
                                type="text"
                                placeholder="Nhập tên (Mẹ, Cha, Anh...)"
                                value={localFilters.face || ""}
                                onChange={e => update({ face: e.target.value || undefined })}
                                className="flex h-9 flex-1 rounded-md border border-input bg-transparent px-3 py-1 text-sm"
                            />
                            {localFilters.face && (
                                <button onClick={() => update({ face: undefined })} className="text-muted-foreground hover:text-foreground px-2">
                                    <X className="w-4 h-4" />
                                </button>
                            )}
                        </div>
                    </div>
                </div>

                <div className="p-4 border-t border-border/10 bg-background flex gap-3">
                    <Button variant="outline" className="flex-1" onClick={handleReset}>
                        Xóa tất cả
                    </Button>
                    <Button className="flex-[2]" onClick={handleApply}>
                        Áp dụng bộ lọc
                        {activeCount > 0 && ` (${activeCount})`}
                    </Button>
                </div>
            </SheetContent>
        </Sheet>
    );
}
