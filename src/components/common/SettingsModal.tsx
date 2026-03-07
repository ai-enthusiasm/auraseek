import {
    Dialog,
    DialogContent,
    DialogDescription,
    DialogHeader,
    DialogTitle,
} from "@/components/ui/dialog";
import { Settings, HardDrive, Cpu, CheckCircle2, AlertCircle, AlertTriangle } from "lucide-react";
import { Button } from "@/components/ui/button";
import { useState, useEffect } from "react";
import { AuraSeekApi } from "@/lib/api";

interface SettingsModalProps {
    open: boolean;
    onOpenChange: (open: boolean) => void;
    currentSourceDir?: string;
    onSourceDirChange?: (dir: string) => void;
}

export function SettingsModal({ open, onOpenChange, currentSourceDir = "", onSourceDirChange }: SettingsModalProps) {
    const [sourceFolder, setSourceFolder] = useState(currentSourceDir);
    const [saving, setSaving] = useState(false);
    const [saved, setSaved] = useState(false);
    const [error, setError] = useState<string | null>(null);
    const [showWarning, setShowWarning] = useState(false);

    useEffect(() => {
        if (open) {
            setSourceFolder(currentSourceDir);
            setSaved(false);
            setError(null);
            setShowWarning(false);
        }
    }, [open, currentSourceDir]);

    const hasChanged = sourceFolder.trim() !== currentSourceDir.trim();

    const handleSave = async () => {
        const trimmed = sourceFolder.trim();
        if (!trimmed) { setError("Vui lòng nhập đường dẫn"); return; }

        if (currentSourceDir && hasChanged && !showWarning) {
            setShowWarning(true);
            return;
        }

        setSaving(true);
        setError(null);
        setShowWarning(false);
        try {
            await AuraSeekApi.setSourceDir(trimmed);
            setSaved(true);
            onSourceDirChange?.(trimmed);
            // Trigger auto-scan with the new folder
            try { await AuraSeekApi.autoScan(); } catch {}
            window.dispatchEvent(new Event("refresh_photos"));
            setTimeout(() => onOpenChange(false), 1200);
        } catch (e) {
            setError(String(e));
        } finally {
            setSaving(false);
        }
    };

    return (
        <Dialog open={open} onOpenChange={onOpenChange}>
            <DialogContent className="sm:max-w-[520px] p-0 overflow-hidden bg-background/95 backdrop-blur-xl shadow-2xl border-border/40 sm:rounded-2xl">
                <DialogHeader className="px-6 py-5 border-b border-border/10 bg-muted/30">
                    <DialogTitle className="flex items-center gap-2 text-xl font-medium tracking-tight text-foreground">
                        <Settings className="w-5 h-5 text-primary" />
                        Cài đặt
                    </DialogTitle>
                    <DialogDescription className="mt-1.5 leading-relaxed">
                        Quản lý thư viện ảnh AuraSeek
                    </DialogDescription>
                </DialogHeader>

                <div className="px-6 py-6 flex flex-col gap-6 overflow-y-auto max-h-[70vh]">
                    {/* Source Folder */}
                    <div className="space-y-3">
                        <h3 className="text-sm font-semibold text-foreground flex items-center gap-2">
                            <HardDrive className="w-4 h-4 text-primary" />
                            Thư mục nguồn ảnh
                        </h3>
                        <p className="text-sm text-muted-foreground leading-relaxed">
                            Thư mục chứa ảnh sẽ được quét, phân tích AI và đồng bộ tự động khi mở ứng dụng.
                        </p>
                        <input
                            type="text"
                            value={sourceFolder}
                            onChange={e => { setSourceFolder(e.target.value); setShowWarning(false); setSaved(false); }}
                            placeholder="/home/user/Pictures"
                            className="flex h-10 w-full rounded-md border border-input bg-transparent px-3 py-1 text-sm font-mono"
                        />

                        {showWarning && (
                            <div className="rounded-xl border border-amber-500/30 bg-amber-500/10 p-3 flex gap-2 text-sm text-amber-700 dark:text-amber-400">
                                <AlertTriangle className="w-4 h-4 shrink-0 mt-0.5" />
                                <div>
                                    <strong>Lưu ý:</strong> Bạn phải di chuyển các ảnh cũ vào thư mục mới nếu muốn thay đổi thư mục nguồn ảnh để đảm bảo trải nghiệm tốt hơn.
                                    <div className="mt-2 flex gap-2">
                                        <button
                                            onClick={handleSave}
                                            className="rounded-md bg-amber-600 hover:bg-amber-500 px-3 py-1 text-xs text-white font-medium"
                                        >
                                            Tôi hiểu, xác nhận
                                        </button>
                                        <button
                                            onClick={() => setShowWarning(false)}
                                            className="rounded-md bg-white/10 hover:bg-white/20 px-3 py-1 text-xs font-medium"
                                        >
                                            Hủy
                                        </button>
                                    </div>
                                </div>
                            </div>
                        )}

                        {!showWarning && (
                            <Button
                                onClick={handleSave}
                                disabled={saving || !sourceFolder.trim()}
                                className="w-full h-10 mt-1 rounded-xl font-medium transition-all shadow-sm"
                            >
                                {saving ? (
                                    <><div className="w-4 h-4 mr-2 border-2 border-white/30 border-t-white rounded-full animate-spin" />Đang lưu...</>
                                ) : saved ? (
                                    <><CheckCircle2 className="w-4 h-4 mr-2" />Đã lưu</>
                                ) : (
                                    "Lưu thư mục nguồn"
                                )}
                            </Button>
                        )}
                    </div>

                    {error && (
                        <div className="rounded-xl border border-destructive/20 bg-destructive/5 p-4 flex gap-2 text-sm text-destructive">
                            <AlertCircle className="w-4 h-4 shrink-0 mt-0.5" />
                            {error}
                        </div>
                    )}

                    {/* About */}
                    <div className="pt-4 border-t border-border/10 flex flex-col items-center justify-center text-center space-y-3">
                        <div className="w-12 h-12 bg-gradient-to-tr from-primary to-indigo-500 rounded-2xl flex items-center justify-center shadow-lg shadow-primary/20">
                            <Cpu className="w-6 h-6 text-white" />
                        </div>
                        <div>
                            <h3 className="text-base font-medium tracking-tight text-foreground">AuraSeek</h3>
                            <p className="text-xs text-muted-foreground mt-0.5">Phiên bản 1.0.0 · SurrealDB + Local AI · Offline</p>
                        </div>
                    </div>
                </div>
            </DialogContent>
        </Dialog>
    );
}
