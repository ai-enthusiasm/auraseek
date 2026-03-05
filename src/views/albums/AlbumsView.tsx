import { Plus, FolderHeart, Camera, Smartphone } from "lucide-react";
import { Button } from "@/components/ui/button";
import type { Photo } from "@/types/photo.type";

export function AlbumsView({ photos = [], onNavigate }: { photos?: Photo[], onNavigate?: (payload: any) => void }) {

    // Compute basic collections based on photo stats
    const favCount = photos.filter(p => p.favorite).length;
    const camCount = photos.filter(p => p.labels?.includes("person") || p.labels?.includes("camera")).length || photos.length; // Fake camera logic
    const scrCount = photos.filter(p => p.labels?.includes("cell phone") || p.labels?.includes("laptop")).length; // Fake screenshot logic

    const collections = [
        { id: "fav", title: "Yêu thích", count: favCount, icon: FolderHeart, coverUrl: "https://picsum.photos/seed/fav/600/400" },
        { id: "cam", title: "Camera", count: camCount, icon: Camera, coverUrl: "https://picsum.photos/seed/cam/600/400" },
        { id: "scr", title: "Ảnh chụp màn hình", count: scrCount, icon: Smartphone, coverUrl: "https://picsum.photos/seed/scr/600/400" },
    ];

    // Compute Custom Albums / Smart Albums based on YOLO AI Tags
    const albumsMap = new Map<string, { id: string; title: string; count: number; coverUrl: string; }>();

    // Map common english labels to Vietnamese for nice display
    const titleMap: Record<string, string> = {
        person: "Con người", dog: "Chó", cat: "Mèo", car: "Ô tô",
        keyboard: "Bàn phím", laptop: "Máy tính", cell_phone: "Điện thoại",
        mouse: "Chuột", cup: "Cốc cà phê", bottle: "Chai nước", book: "Sách",
        motorcycle: "Xe máy", airplane: "Máy bay", bus: "Xe buýt", truck: "Xe tải",
        bird: "Chim", horse: "Ngựa", sheep: "Cừu", cow: "Bò", elephant: "Voi",
        bear: "Gấu", zebra: "Ngựa vằn", giraffe: "Hươu cao cổ", backpack: "Balo",
        umbrella: "Cái ô", handbag: "Túi xách", tie: "Cà vạt", suitcase: "Vali",
        frisbee: "Đĩa bay", skis: "Ván trượt", snowboard: "Ván trượt tuyết", sports_ball: "Quả bóng",
        kite: "Cái diều", baseball_bat: "Gậy bóng chày", baseball_glove: "Găng tay bóng chày",
        skateboard: "Trượt ván", surfboard: "Ván lướt sóng", tennis_racket: "Vợt tennis",
        bottle_water: "Chai nước", wine_glass: "Ly rượu", cup_coffee: "Tách cà phê", fork: "Cái nĩa",
        knife: "Cái dao", spoon: "Cái thìa", bowl: "Cái bát", banana: "Quả chuối",
        apple: "Quả táo", sandwich: "Bánh mì kẹp", orange: "Quả cam", broccoli: "Súp lơ",
        carrot: "Củ cà rốt", hot_dog: "Xúc xích", pizza: "Bánh pizza", donut: "Bánh donut",
        cake: "Bánh ngọt", chair: "Cái ghế", couch: "Ghế sofa", potted_plant: "Chậu cây",
        bed: "Cái giường", dining_table: "Bàn ăn", toilet: "Bồn cầu", tv: "Tivi",
        remote: "Điều khiển", microwave: "Lò vi sóng", oven: "Lò nướng", toaster: "Máy nướng bánh",
        sink: "Bồn rửa", refrigerator: "Tủ lạnh", clock: "Cái đồng hồ", vase: "Cái bình",
        scissors: "Cái kéo", teddy_bear: "Gấu bông", hair_drier: "Máy sấy tóc", toothbrush: "Bàn chải",
    };

    for (const p of photos) {
        if (!p.labels) continue;
        for (const label of p.labels) {
            const normalizedTag = label.toLowerCase();
            const title = titleMap[normalizedTag] || label;

            if (!albumsMap.has(normalizedTag)) {
                albumsMap.set(normalizedTag, {
                    id: "tag_" + normalizedTag,
                    title,
                    count: 0,
                    coverUrl: p.url
                });
            }
            albumsMap.get(normalizedTag)!.count++;
        }
    }

    const customAlbums = Array.from(albumsMap.values())
        .sort((a, b) => b.count - a.count);

    return (
        <div className="flex-1 overflow-y-auto px-6 py-8 will-change-scroll">
            <div className="max-w-7xl mx-auto space-y-12">

                {/* Header */}
                <div className="flex items-center justify-between">
                    <h1 className="text-2xl font-medium tracking-tight">Bộ sưu tập</h1>
                </div>

                {/* Default Collections */}
                <div className="grid grid-cols-2 sm:grid-cols-3 md:grid-cols-4 lg:grid-cols-5 gap-4">
                    {collections.map(col => (
                        <div key={col.id} className="group cursor-pointer" onClick={() => onNavigate?.({ id: col.id, title: col.title })}>
                            <div className="aspect-square rounded-xl overflow-hidden bg-muted mb-3 transition-all duration-300 ring-2 ring-transparent group-hover:ring-primary shadow-sm group-hover:shadow-md relative">
                                <img src={col.coverUrl} className="w-full h-full object-cover transition-transform duration-500 ease-out group-hover:scale-105" />
                                <div className="absolute inset-0 bg-gradient-to-t from-black/60 via-black/0 to-black/0" />
                                <col.icon className="absolute bottom-3 left-3 w-5 h-5 text-white" />
                            </div>
                            <div className="font-medium text-sm truncate px-1">{col.title}</div>
                            <div className="text-xs text-muted-foreground px-1">{col.count} mục</div>
                        </div>
                    ))}
                </div>

                {/* Custom Albums */}
                <div>
                    <div className="flex items-center justify-between mb-6">
                        <h2 className="text-lg font-medium">Album của bạn</h2>
                        <Button variant="outline" size="sm" className="rounded-full shadow-sm text-xs font-medium h-9">
                            <Plus className="w-4 h-4 mr-1" />
                            Tạo album mới
                        </Button>
                    </div>

                    <div className="grid grid-cols-2 sm:grid-cols-3 md:grid-cols-4 lg:grid-cols-5 xl:grid-cols-6 gap-x-4 gap-y-8">
                        {customAlbums.map(album => (
                            <div key={album.id} className="group cursor-pointer" onClick={() => onNavigate?.({ id: album.id, title: album.title })}>
                                <div className="aspect-[4/3] rounded-xl overflow-hidden bg-muted mb-3 transition-all duration-300 ring-2 ring-transparent group-hover:ring-primary shadow-sm group-hover:shadow-md">
                                    <img src={album.coverUrl} className="w-full h-full object-cover transition-transform duration-500 ease-out group-hover:scale-105" />
                                </div>
                                <div className="font-medium text-sm truncate px-1">{album.title}</div>
                                <div className="text-xs text-muted-foreground px-1">{album.count} mục</div>
                            </div>
                        ))}
                    </div>
                </div>

            </div>
        </div>
    );
}
