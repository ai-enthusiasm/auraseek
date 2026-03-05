export type PhotoLocation = {
  latitude?: number;
  longitude?: number;
  address?: string;
  placeName?: string;
};

export type PhotoPersonRef = {
  id: string;
  name: string;
};

export type Photo = {
  id: string;
  url: string;
  takenAt: string;
  createdAt: string;
  sizeBytes: number;
  width: number;
  height: number;
  cameraModel?: string;
  iso?: number;
  focalLength?: number;
  location?: PhotoLocation;
  people?: PhotoPersonRef[];
  objects?: string[];
  textSnippets?: string[];
  favorite?: boolean;
  duplicateGroupId?: string;
  labels?: string[];
  faces?: string[];
  type?: "photo" | "video";
};

