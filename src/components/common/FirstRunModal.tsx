import { useState } from "react";
import { createPortal } from "react-dom";
import { Folder, Check, RefreshCcw, Sparkles } from "lucide-react";
import { AuraSeekApi } from "@/lib/api";
import { cn } from "@/lib/utils";

interface FirstRunModalProps {
  onComplete: (dir: string) => void;
}

export function FirstRunModal({ onComplete }: FirstRunModalProps) {
  const [dir, setDir] = useState("");
  const [saving, setSaving] = useState(false);
  const [error, setError] = useState("");

  const isWindows = typeof navigator !== 'undefined' && navigator.userAgent.toLowerCase().includes("win");
  const placeholderTxt = isWindows ? "C:\\Users\\PhuocDai\\Pictures" : "/Users/phuocdai/Pictures";

  const handleConfirm = async () => {
    const trimmed = dir.trim();
    if (!trimmed) {
      setError("Vui lòng nhập đường dẫn thư mục");
      return;
    }
    setSaving(true);
    setError("");
    try {
      await AuraSeekApi.setSourceDir(trimmed);
      onComplete(trimmed);
    } catch (e) {
      setError(String(e));
    } finally {
      setSaving(false);
    }
  };

  const handleReload = () => {
    window.location.reload();
  };

  const shell = (
    <div
      className="fixed inset-0 z-[200000] flex items-center justify-center p-6"
      data-auraseek-first-run-overlay
    >
      {/* Heavy Backdrop Blur */}
      <div className="absolute inset-0 bg-black/40 backdrop-blur-2xl transition-all duration-700" onClick={(e) => e.stopPropagation()} />

      <div className="relative w-full max-w-xl overflow-hidden rounded-[40px] bg-[#0c0c14]/80 border border-white/10 shadow-[0_32px_120px_rgba(0,0,0,0.6)] backdrop-blur-3xl animate-in zoom-in-95 duration-500">
        
        {/* Decorative Top Glow */}
        <div className="absolute top-0 left-1/2 -translate-x-1/2 w-full h-[1px] bg-gradient-to-r from-transparent via-indigo-500/50 to-transparent" />
        <div className="absolute -top-24 left-1/2 -translate-x-1/2 w-64 h-32 bg-indigo-600/20 blur-[60px] rounded-full" />

        <div className="px-10 py-12 flex flex-col items-center">
          
          <div className="relative mb-8 group">
            <div className="absolute inset-0 bg-indigo-500/20 blur-2xl rounded-full scale-150 group-hover:scale-[2] transition-transform duration-700 opacity-50" />
            <div className="relative flex h-20 w-20 items-center justify-center rounded-3xl bg-gradient-to-br from-indigo-500 to-purple-600 shadow-[0_10px_30px_rgba(99,102,241,0.4)] border border-white/20">
              <Sparkles className="w-10 h-10 text-white animate-pulse" />
            </div>
          </div>

          <h2 className="text-3xl font-bold text-white tracking-tight text-center mb-3">
            Bắt đầu trải nghiệm AuraSeek
          </h2>
          <p className="text-gray-400 text-center max-w-sm mb-10 leading-relaxed">
            Chọn một thư mục ảnh để chúng em bắt đầu "phép màu" AI — phân tích khuôn mặt và tìm kiếm thông minh.
          </p>

          <div className="w-full space-y-6">
            <div className="space-y-3">
              <div className="flex justify-between items-center px-2">
                 <label className="text-[10px] font-black text-white/40 uppercase tracking-[0.2em]">
                   Thư mục nguồn ảnh
                 </label>
                 <button onClick={handleReload} className="text-[10px] font-bold text-indigo-400/80 hover:text-indigo-400 flex items-center gap-1.5 transition-colors uppercase tracking-wider">
                   <RefreshCcw className="w-3 h-3" /> Làm mới
                 </button>
              </div>
              
              <div className="relative group">
                <div className="absolute inset-y-0 left-4 flex items-center pointer-events-none">
                  <Folder className="w-5 h-5 text-gray-500 group-focus-within:text-indigo-400 transition-colors" />
                </div>
                <input
                  type="text"
                  value={dir}
                  onChange={(e) => setDir(e.target.value)}
                  onKeyDown={(e) => e.key === "Enter" && handleConfirm()}
                  placeholder={placeholderTxt}
                  className={cn(
                    "w-full h-16 pl-12 pr-6 rounded-2xl bg-white/5 border border-white/10 text-lg text-white placeholder:text-white/20",
                    "focus:outline-none focus:ring-4 focus:ring-indigo-500/10 focus:border-indigo-500/40 transition-all duration-300 shadow-inner",
                    error ? "border-red-500/40 focus:border-red-500/40 focus:ring-red-500/10" : ""
                  )}
                />
              </div>
              {error && (
                <div className="px-2 flex items-center gap-2 text-red-400 text-xs font-medium animate-in slide-in-from-top-1">
                   <span className="w-1 h-1 rounded-full bg-red-400 shadow-[0_0_8px_rgba(248,113,113,0.8)]" />
                   {error}
                </div>
              )}
            </div>

            <button
              onClick={handleConfirm}
              disabled={saving || !dir.trim()}
              className={cn(
                "group relative w-full h-16 overflow-hidden rounded-2xl font-bold text-lg transition-all active:scale-[0.98]",
                "bg-gradient-to-r from-indigo-600 via-indigo-500 to-indigo-600 bg-[length:200%_100%] hover:bg-right",
                "text-white shadow-[0_10px_40px_rgba(79,70,229,0.3)] hover:shadow-[0_15px_50px_rgba(79,70,229,0.5)]",
                "disabled:opacity-40 disabled:hover:bg-left disabled:cursor-not-allowed disabled:shadow-none"
              )}
            >
              <div className="relative flex items-center justify-center gap-3">
                {saving ? (
                  <span className="h-5 w-5 animate-spin rounded-full border-2 border-white/30 border-t-white" />
                ) : (
                  <>
                    <span>Đồng ý & Bắt đầu</span>
                    <Check className="w-5 h-5 group-hover:scale-125 transition-transform" />
                  </>
                )}
              </div>
            </button>

            <p className="text-[11px] text-white/20 text-center leading-normal">
              AuraSeek thu thập dữ liệu cục bộ. Chúng em không bao giờ tải ảnh tài liệu của bạn lên bất kỳ máy chủ nào.
            </p>
          </div>
        </div>
      </div>
    </div>
  );

  if (typeof document === "undefined") return null;
  return createPortal(shell, document.body);
}
