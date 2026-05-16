import { useState, useEffect } from "react";
import { Button } from "@/components/ui/button";
import { AuraSeekApi } from "@/lib/api";
import { Plus, Loader2 } from "lucide-react";

interface AddToAlbumDialogProps {
  isOpen: boolean;
  onClose: () => void;
  mediaIds: string[];
  onSuccess?: () => void;
}

export function AddToAlbumDialog({ isOpen, onClose, mediaIds, onSuccess }: AddToAlbumDialogProps) {
  const [albums, setAlbums] = useState<{ id: string; title: string; count: number }[]>([]);
  const [isLoading, setIsLoading] = useState(false);
  const [isSaving, setIsSaving] = useState(false);
  const [showNewAlbumInput, setShowNewAlbumInput] = useState(false);
  const [newAlbumTitle, setNewAlbumTitle] = useState("");

  useEffect(() => {
    if (isOpen) {
      setShowNewAlbumInput(false);
      setNewAlbumTitle("");
      setIsLoading(true);
      AuraSeekApi.getAlbums()
        .then(setAlbums)
        .catch(console.error)
        .finally(() => setIsLoading(false));
    }
  }, [isOpen]);

  if (!isOpen) return null;

  const handleAddToExisting = async (albumId: string) => {
    try {
      setIsSaving(true);
      await AuraSeekApi.addToAlbum(albumId, mediaIds);
      onSuccess?.();
      onClose();
    } catch (e) {
      console.error(e);
      alert("Lỗi khi thêm vào album");
    } finally {
      setIsSaving(false);
    }
  };

  const handleCreateAndAdd = async () => {
    if (!newAlbumTitle.trim()) return;
    try {
      setIsSaving(true);
      const albumId = await AuraSeekApi.createAlbum(newAlbumTitle.trim());
      await AuraSeekApi.addToAlbum(albumId, mediaIds);
      onSuccess?.();
      onClose();
    } catch (e) {
      console.error(e);
      alert("Lỗi khi tạo album mới");
    } finally {
      setIsSaving(false);
    }
  };

  return (
    <div className="fixed inset-0 z-[100] flex items-center justify-center bg-background/80 backdrop-blur-sm animate-in fade-in duration-200">
      <div className="bg-card w-full max-w-sm rounded-2xl shadow-xl border border-border/50 overflow-hidden flex flex-col animate-in zoom-in-95 duration-200">
        <div className="p-5 pb-4">
          <h2 className="text-xl font-semibold tracking-tight mb-1">Thêm vào album</h2>
          <p className="text-sm text-muted-foreground">Chọn album để thêm {mediaIds.length} mục vào</p>
        </div>

        <div className="max-h-[50vh] overflow-y-auto px-2 pb-2">
          {isLoading ? (
            <div className="flex justify-center py-8 text-muted-foreground"><Loader2 className="w-6 h-6 animate-spin" /></div>
          ) : albums.length === 0 && !showNewAlbumInput ? (
            <div className="text-center py-6 text-sm text-muted-foreground">Chưa có album nào.</div>
          ) : (
            <div className="flex flex-col gap-1">
              {!showNewAlbumInput && albums.map(a => (
                <button
                  key={a.id}
                  disabled={isSaving}
                  onClick={() => handleAddToExisting(a.id)}
                  className="flex items-center justify-between w-full px-4 py-3 rounded-xl hover:bg-muted/50 text-left transition-colors disabled:opacity-50"
                >
                  <span className="font-medium text-sm truncate">{a.title}</span>
                  <span className="text-xs text-muted-foreground ml-2 shrink-0">{a.count} mục</span>
                </button>
              ))}
            </div>
          )}

          {showNewAlbumInput ? (
            <div className="px-3 py-2 flex flex-col gap-3">
              <input
                autoFocus
                type="text"
                placeholder="Tên album mới..."
                value={newAlbumTitle}
                onChange={e => setNewAlbumTitle(e.target.value)}
                onKeyDown={e => {
                  if (e.key === "Enter") handleCreateAndAdd();
                  if (e.key === "Escape") setShowNewAlbumInput(false);
                }}
                className="w-full bg-muted/50 border border-border rounded-lg px-3 py-2 text-sm outline-none focus:border-primary transition-colors"
                disabled={isSaving}
              />
              <div className="flex justify-end gap-2">
                <Button variant="ghost" size="sm" onClick={() => setShowNewAlbumInput(false)} disabled={isSaving}>Hủy</Button>
                <Button size="sm" onClick={handleCreateAndAdd} disabled={!newAlbumTitle.trim() || isSaving}>
                  {isSaving ? <Loader2 className="w-4 h-4 mr-2 animate-spin" /> : null}
                  Tạo & Thêm
                </Button>
              </div>
            </div>
          ) : (
            <button
              onClick={() => setShowNewAlbumInput(true)}
              className="flex items-center gap-2 w-full px-4 py-3 rounded-xl hover:bg-primary/5 text-primary text-sm font-medium transition-colors mt-2"
              disabled={isSaving}
            >
              <Plus className="w-4 h-4" /> Tạo album mới
            </button>
          )}
        </div>

        <div className="p-4 pt-2 flex justify-end">
          <Button variant="outline" onClick={onClose} disabled={isSaving} className="rounded-full w-full">Đóng</Button>
        </div>
      </div>
    </div>
  );
}
