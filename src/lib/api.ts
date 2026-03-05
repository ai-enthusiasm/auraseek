import { invoke } from "@tauri-apps/api/core";

// ─── Types ────────────────────────────────────────────────────────────────────

export interface SearchResultMeta {
    width: number | null;
    height: number | null;
    created_at: string | null;
    objects: string[];
    faces: string[];
}

export interface SearchResult {
    media_id: string;
    similarity_score: number;
    file_path: string;
    media_type: string;
    metadata: SearchResultMeta;
}

export interface TimelineItem {
    media_id: string;
    file_path: string;
    media_type: string;
    width: number | null;
    height: number | null;
    created_at: string | null;
    objects: string[];
    faces: string[];
}

export interface TimelineGroup {
    label: string;
    year: number;
    month: number;
    day: number | null;
    items: TimelineItem[];
}

export interface PersonGroup {
    face_id: string;
    name: string | null;
    photo_count: number;
    cover_path: string | null;
    thumbnail: string | null;
}

export interface DuplicateItem {
    media_id: string;
    file_path: string;
    size: number;
}

export interface DuplicateGroup {
    sha256: string;
    items: DuplicateItem[];
}

export interface IngestSummary {
    total_found: number;
    newly_added: number;
    skipped_dup: number;
    errors: number;
}

export interface SearchFilters {
    object?: string;
    face?: string;
    month?: number;
    year?: number;
    media_type?: string;
}

export interface AppStatus {
    engine_ready: boolean;
    db_ready: boolean;
    vector_count: number;
}

// ─── API ─────────────────────────────────────────────────────────────────────

export const AuraSeekApi = {
    /** Initialize AI engine and DB. Must be called first. */
    async init(): Promise<string> {
        return invoke<string>("cmd_init");
    },

    /** Get engine + DB status */
    async getStatus(): Promise<AppStatus> {
        return invoke<AppStatus>("cmd_get_status");
    },

    /** Scan a folder for images/videos */
    async scanFolder(sourcePath: string): Promise<IngestSummary> {
        return invoke<IngestSummary>("cmd_scan_folder", { sourcePath });
    },

    /** Text search */
    async searchText(query: string, filters?: SearchFilters): Promise<SearchResult[]> {
        return invoke<SearchResult[]>("cmd_search_text", { query, filters });
    },

    /** Image search (by file path) */
    async searchImage(imagePath: string, filters?: SearchFilters): Promise<SearchResult[]> {
        return invoke<SearchResult[]>("cmd_search_image", { imagePath, filters });
    },

    /** Combined text + image search */
    async searchCombined(text: string, imagePath: string, filters?: SearchFilters): Promise<SearchResult[]> {
        return invoke<SearchResult[]>("cmd_search_combined", { text, imagePath, filters });
    },

    /** Search by COCO object class */
    async searchObject(className: string): Promise<SearchResult[]> {
        return invoke<SearchResult[]>("cmd_search_object", { className });
    },

    /** Search by person name */
    async searchFace(name: string): Promise<SearchResult[]> {
        return invoke<SearchResult[]>("cmd_search_face", { name });
    },

    /** Get timeline grouped by month */
    async getTimeline(limit?: number): Promise<TimelineGroup[]> {
        return invoke<TimelineGroup[]>("cmd_get_timeline", { limit });
    },

    /** Get all recognized people */
    async getPeople(): Promise<PersonGroup[]> {
        return invoke<PersonGroup[]>("cmd_get_people");
    },

    /** Name a face cluster */
    async namePerson(faceId: string, name: string): Promise<void> {
        return invoke<void>("cmd_name_person", { faceId, name });
    },

    /** Get duplicate image groups */
    async getDuplicates(): Promise<DuplicateGroup[]> {
        return invoke<DuplicateGroup[]>("cmd_get_duplicates");
    },

    /** Get recent search history */
    async getSearchHistory(limit?: number): Promise<any[]> {
        return invoke<any[]>("cmd_get_search_history", { limit });
    },

    /** Set MongoDB URI */
    async setMongoUri(uri: string): Promise<void> {
        return invoke<void>("cmd_set_mongo_uri", { uri });
    },
};

/** Convert a local file path to a displayable URL in Tauri.
 *  Uses the asset:// protocol to serve local files. */
export function localFileUrl(filePath: string): string {
    if (!filePath) return "";
    // Tauri v2 can serve local files via the asset protocol
    // Encode the path for use in a URL
    const encoded = encodeURIComponent(filePath).replace(/%2F/g, "/").replace(/%5C/g, "/");
    return `asset://localhost/${encoded}`;
}
