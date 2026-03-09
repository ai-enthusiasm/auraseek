import { useState, useEffect } from "react";
import { Button } from "@/components/ui/button";
import { Copy, Trash2, CheckCircle2 } from "lucide-react";
import { AuraSeekApi, type DuplicateGroup, localFileUrl } from "@/lib/api";

export function DuplicatesView() {
    const [groups, setGroups] = useState<DuplicateGroup[]>([]);
    const [isLoading, setIsLoading] = useState(true);

    useEffect(() => {
        const fetchDuplicates = async () => {
            try {
                const data = await AuraSeekApi.getDuplicates();
                setGroups(data);
            } catch (err) {
                console.error("Failed to fetch duplicates:", err);
            } finally {
                setIsLoading(false);
            }
        };
        fetchDuplicates();
    }, []);

    const { totalSavingsMB, totalDiscardCount } = groups.reduce((acc, current) => {
        let sizeSaved = 0;
        let discard = 0;
        current.items.forEach((item, i) => {
            if (i > 0) { // Keep the first (i===0), discard the rest
                sizeSaved += item.size;
                discard += 1;
            }
        });
        return { 
            totalSavingsMB: acc.totalSavingsMB + (sizeSaved / 1024 / 1024),
            totalDiscardCount: acc.totalDiscardCount + discard
        };
    }, { totalSavingsMB: 0, totalDiscardCount: 0 });

    return (
        <div className="flex-1 overflow-y-auto px-6 py-8 bg-background/50">
            <div className="max-w-4xl mx-auto space-y-8">

                {/* Header */}
                <div className="flex flex-col gap-2">
                    <div className="w-12 h-12 rounded-full bg-primary/10 flex items-center justify-center mb-2">
                        <Copy className="w-6 h-6 text-primary" />
                    </div>
                    <h1 className="text-2xl font-medium tracking-tight">Dọn dẹp ảnh trùng lặp</h1>
                    <p className="text-muted-foreground text-sm max-w-xl">
                        Hệ thống tự động phát hiện các ảnh giống hệt hoặc tương tự nhau bằng model AI. Chúng tôi đề xuất giữ lại tấm nét nhất, nguyên gốc và xóa các tấm mờ/dung lượng thấp để tiết kiệm không gian.
                    </p>
                </div>

                {totalDiscardCount > 0 && (
                    <div className="flex items-center justify-between p-4 bg-muted/40 rounded-xl border border-border/20">
                        <div>
                            <div className="font-medium text-sm">Có thể tiết kiệm {totalSavingsMB.toFixed(1)} MB</div>
                            <div className="text-xs text-muted-foreground mt-0.5">Bằng cách dọn dẹp các mục được đề xuất xóa ({totalDiscardCount} mục)</div>
                        </div>
                        <Button className="rounded-full shadow-sm text-sm" size="sm">
                            Xóa tất cả {totalDiscardCount} mục thừa
                        </Button>
                    </div>
                )}

                {/* Groups */}
                {isLoading ? (
                    <div className="text-center text-muted-foreground py-10">Đang quét dữ liệu trùng lặp...</div>
                ) : groups.length === 0 ? (
                    <div className="text-center text-muted-foreground py-10">
                        <div className="text-4xl mb-3">✨</div>
                        <div className="font-medium text-lg text-foreground">Không tìm thấy ảnh trùng lặp</div>
                        <div className="text-sm">Thư viện của bạn đang rất gọn gàng.</div>
                    </div>
                ) : (
                    <div className="space-y-6">
                        {groups.map((group, idx) => {
                            // Automatically recommend keeping the first one, discarding the rest
                            const groupWithKeepScore = group.items.map((item, i) => ({
                                ...item,
                                keep: i === 0, // Keep the first item arbitrarily as best match for now
                                formattedSize: (item.size / 1024 / 1024).toFixed(1) + " MB"
                            }));

                            const discardCount = groupWithKeepScore.filter(p => !p.keep).length;

                            return (
                                <div key={idx} className="bg-background rounded-2xl border border-border/30 overflow-hidden shadow-sm">
                                    <div className="flex items-center justify-between px-5 py-4 border-b border-border/10 bg-muted/10">
                                        <span className="font-medium text-sm font-mono text-muted-foreground" title={group.group_id}>
                                            Lý do: {group.reason}
                                        </span>
                                        <Button variant="outline" size="sm" className="h-8 rounded-full text-xs hover:bg-destructive/10 hover:text-destructive hover:border-destructive/30 transition-colors">
                                            <Trash2 className="w-4 h-4 mr-2" />
                                            Đã chọn {discardCount} ảnh
                                        </Button>
                                    </div>

                                    <div className="p-5 flex gap-4 overflow-x-auto snap-x">
                                        {groupWithKeepScore.map((photo, pIdx) => (
                                            <div key={pIdx} className="snap-start relative w-48 shrink-0 flex flex-col gap-2">
                                                <div className={`aspect-4/3 rounded-xl overflow-hidden cursor-pointer transition-all ${photo.keep ? 'ring-2 ring-emerald-500 ring-offset-2 ring-offset-background' : 'ring-2 ring-destructive ring-offset-2 ring-offset-background opacity-80 hover:opacity-100'}`}>
                                                    <img src={localFileUrl(photo.file_path)} className="w-full h-full object-cover" />
                                                    <div className="absolute top-2 left-2 z-10 flex gap-2">
                                                        {photo.keep ? (
                                                            <span className="bg-emerald-500/90 text-white backdrop-blur px-2 py-0.5 rounded-full text-[10px] font-medium flex items-center gap-1 shadow-sm">
                                                                <CheckCircle2 className="w-3 h-3" />
                                                                GIỮ LẠI
                                                            </span>
                                                        ) : (
                                                            <span className="bg-destructive/90 text-white backdrop-blur px-2 py-0.5 rounded-full text-[10px] font-medium flex items-center gap-1 shadow-sm">
                                                                <Trash2 className="w-3 h-3" />
                                                                ĐỀ XUẤT XÓA
                                                            </span>
                                                        )}
                                                    </div>
                                                    <div className="absolute bottom-2 right-2 bg-black/60 text-white backdrop-blur px-1.5 py-0.5 rounded text-[10px] shadow-sm">
                                                        {photo.formattedSize}
                                                    </div>
                                                </div>
                                            </div>
                                        ))}
                                    </div>
                                </div>
                            );
                        })}
                    </div>
                )}

                <div className="h-20" />
            </div>
        </div>
    );
}
