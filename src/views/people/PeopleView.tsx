import { useState } from "react";
import type { Photo } from "@/types/photo.type";
import type { PersonGroup } from "@/lib/api";
import { localFileUrl, AuraSeekApi } from "@/lib/api";
import { Pencil, Check, X } from "lucide-react";

interface PeopleViewProps {
    photos?: Photo[];
    people?: PersonGroup[];
    onNavigate?: (payload: any) => void;
}

function PersonCard({
    person,
    onNavigate,
    onRename,
}: {
    person: PersonGroup;
    onNavigate?: (payload: any) => void;
    onRename?: (faceId: string, name: string) => void;
}) {
    const [editing, setEditing] = useState(false);
    const [name, setName] = useState(person.name || "");

    const handleSave = async () => {
        if (!name.trim()) return;
        try {
            await AuraSeekApi.namePerson(person.face_id, name.trim());
            onRename?.(person.face_id, name.trim());
        } catch (e) {
            console.error("Failed to name person:", e);
        }
        setEditing(false);
    };

    const displayName = person.name || `Người ${person.face_id.substring(0, 4).toUpperCase()}`;
    const coverUrl = person.cover_path
        ? localFileUrl(person.cover_path)
        : `https://api.dicebear.com/7.x/thumbs/svg?seed=${person.face_id}`;

    return (
        <div className="group flex flex-col items-center gap-3">
            <div
                className="w-24 h-24 sm:w-28 sm:h-28 rounded-full overflow-hidden bg-muted transition-all duration-300 ring-2 ring-transparent group-hover:ring-primary shadow-sm hover:shadow-md cursor-pointer"
                onClick={() => onNavigate?.({ id: person.face_id, title: displayName })}
            >
                <img
                    src={coverUrl}
                    alt={displayName}
                    className="w-full h-full object-cover transition-transform duration-500 ease-out group-hover:scale-110"
                    onError={(e) => {
                        (e.target as HTMLImageElement).src = `https://api.dicebear.com/7.x/thumbs/svg?seed=${person.face_id}`;
                    }}
                />
            </div>

            <div className="flex flex-col items-center gap-0.5 w-full px-1">
                {editing ? (
                    <div className="flex items-center gap-1 w-full justify-center">
                        <input
                            type="text"
                            value={name}
                            onChange={e => setName(e.target.value)}
                            onKeyDown={e => { if (e.key === "Enter") handleSave(); if (e.key === "Escape") setEditing(false); }}
                            className="w-24 text-xs border border-primary/30 rounded px-1.5 py-0.5 text-center bg-background outline-none focus:border-primary"
                            autoFocus
                        />
                        <button onClick={handleSave} className="text-emerald-500 hover:text-emerald-400"><Check className="w-3.5 h-3.5" /></button>
                        <button onClick={() => setEditing(false)} className="text-muted-foreground hover:text-foreground"><X className="w-3.5 h-3.5" /></button>
                    </div>
                ) : (
                    <div className="flex items-center gap-1 group/name">
                        <span className="text-sm font-medium truncate max-w-[100px] text-center">{displayName}</span>
                        <button
                            onClick={() => { setName(person.name || ""); setEditing(true); }}
                            className="opacity-0 group-hover/name:opacity-100 text-muted-foreground hover:text-primary transition-opacity"
                        >
                            <Pencil className="w-3 h-3" />
                        </button>
                    </div>
                )}
                <span className="text-xs text-muted-foreground">{person.photo_count} ảnh</span>
            </div>
        </div>
    );
}

export function PeopleView({ photos = [], people = [], onNavigate }: PeopleViewProps) {
    const [localPeople, setLocalPeople] = useState<PersonGroup[]>(people);

    const handleRename = (faceId: string, name: string) => {
        setLocalPeople(prev =>
            prev.map(p => p.face_id === faceId ? { ...p, name } : p)
        );
    };

    const displayPeople = localPeople.length > 0 ? localPeople : people;

    // Fallback: derive from photos if no people from DB
    const legacyPeople = photos.reduce((acc, photo) => {
        photo.faces?.forEach((faceId) => {
            if (!acc[faceId]) {
                acc[faceId] = {
                    face_id: faceId,
                    name: null,
                    photo_count: 0,
                    cover_path: photo.url,
                    thumbnail: null,
                };
            }
            acc[faceId].photo_count++;
        });
        return acc;
    }, {} as Record<string, PersonGroup>);

    const allPeople = displayPeople.length > 0
        ? displayPeople
        : Object.values(legacyPeople).sort((a, b) => b.photo_count - a.photo_count);

    return (
        <div className="flex-1 overflow-y-auto px-6 py-8 will-change-scroll">
            <div className="max-w-7xl mx-auto space-y-8">
                <div>
                    <h1 className="text-2xl font-medium tracking-tight">Người và thú cưng</h1>
                    <p className="text-muted-foreground mt-1 text-sm">
                        Tự động nhóm khuôn mặt bằng AI. Click vào tên để đặt tên dễ nhớ hơn.
                    </p>
                </div>

                {allPeople.length === 0 ? (
                    <div className="text-center py-20 text-muted-foreground opacity-60">
                        <div className="text-5xl mb-4">🤖</div>
                        <p className="font-medium">Chưa có khuôn mặt nào được nhận diện</p>
                        <p className="text-sm mt-1">Hãy quét thư viện ảnh để AI nhận diện khuôn mặt tự động</p>
                    </div>
                ) : (
                    <div className="grid grid-cols-2 min-[500px]:grid-cols-3 sm:grid-cols-4 md:grid-cols-5 lg:grid-cols-6 xl:grid-cols-8 gap-y-8 gap-x-4">
                        {allPeople.map((person) => (
                            <PersonCard
                                key={person.face_id}
                                person={person}
                                onNavigate={onNavigate}
                                onRename={handleRename}
                            />
                        ))}
                    </div>
                )}
            </div>
        </div>
    );
}
