# TÀI LIỆU KỸ THUẬT HỆ THỐNG AURASEEK

> **Phiên bản:** 1.0.0  
> **Ngày:** Tháng 3 năm 2026  
> **Tác giả:** AI Enthusiasm  
> **Ngôn ngữ:** Rust (Backend) + TypeScript/React (Frontend)  
> **Nền tảng:** Tauri v2 (Desktop Application)

---

## MỤC LỤC

- [Chương 1: Tổng quan hệ thống](#chương-1-tổng-quan-hệ-thống)
- [Chương 2: Cơ sở lý thuyết và phương pháp đề xuất](#chương-2-cơ-sở-lý-thuyết-và-phương-pháp-đề-xuất)
- [Chương 3: Dữ liệu để huấn luyện và tinh chỉnh](#chương-3-dữ-liệu-để-huấn-luyện-và-tinh-chỉnh)
- [Chương 4: Đánh giá hiệu suất của mô hình](#chương-4-đánh-giá-hiệu-suất-của-mô-hình)
- [Chương 5: Thiết kế phần mềm và triển khai kỹ thuật](#chương-5-thiết-kế-phần-mềm-và-triển-khai-kỹ-thuật)
- [Chương 6: Thực nghiệm, kiểm thử và đánh giá hiệu suất](#chương-6-thực-nghiệm-kiểm-thử-và-đánh-giá-hiệu-suất)
- [Chương 7: Các thách thức và định hướng mở rộng](#chương-7-các-thách-thức-và-định-hướng-mở-rộng)

---

# Chương 1: Tổng quan hệ thống

## 1.1. Giới thiệu về AuraSeek

**AuraSeek** là một ứng dụng quản lý và tìm kiếm ảnh/video thông minh chạy trên nền tảng desktop, được phát triển bằng nền tảng **Tauri v2** kết hợp giữa frontend **React + TypeScript** và backend **Rust**. Điểm đặc biệt của AuraSeek là khả năng tích hợp pipeline AI hoàn chỉnh trực tiếp trên máy người dùng (**on-device AI**), không phụ thuộc vào dịch vụ đám mây bên ngoài.

Tên `AuraSeek` gợi lên ý tưởng "tìm kiếm bầu khí" — hệ thống có khả năng hiểu ngữ nghĩa sâu của ảnh và tìm kiếm theo nội dung thực sự, không chỉ dựa trên tên file hay metadata thông thường.

## 1.2. Mục tiêu hệ thống

AuraSeek được thiết kế với các mục tiêu chính:

1. **Tìm kiếm ngữ nghĩa (Semantic Search):** Cho phép người dùng tìm ảnh bằng từ ngữ tự nhiên (tiếng Việt hoặc tiếng Anh), ví dụ: *"ảnh gia đình bên biển"*, *"xe máy trên đường phố"*.
2. **Nhận diện đối tượng tự động:** Phát hiện và phân loại hơn 80 class đối tượng phổ biến (người, xe, động vật, đồ vật...) trong từng ảnh/video.
3. **Phát hiện và nhận diện khuôn mặt:** Tự động phát hiện khuôn mặt, nhóm theo danh tính (clustering), hỗ trợ người dùng đặt tên cho từng nhóm.
4. **Timeline và quản lý bộ sưu tập:** Hiển thị ảnh/video theo trục thời gian, hỗ trợ album, yêu thích, thùng rác, ẩn ảnh.
5. **Phát hiện ảnh trùng lặp (Duplicate Detection):** Tự động phát hiện ảnh/video trùng nhau theo nhiều phương pháp: SHA-256, pHash, vector embedding similarity.
6. **Tìm kiếm bằng ảnh (Image-to-Image Search):** Người dùng tải lên một ảnh làm query, hệ thống trả về các ảnh có nội dung tương tự trong bộ sưu tập.
7. **Xử lý video:** Phân tích video theo cảnh (scene detection), trích xuất frame đại diện, nhận diện đối tượng và khuôn mặt trong video.

## 1.3. Các chức năng chính

| Chức năng | Mô tả | Module chính |
|-----------|-------|--------------|
| Import ảnh | Quét folder, drag-drop, clipboard paste | `ingest/image_ingest.rs` |
| Import video | Phân tích video, scene detection | `ingest/video_ingest.rs` |
| Object Detection | YOLO segmentation (80+ classes) | `model/yolo.rs` |
| Face Detection | YuNet detector (OpenCV) | `model/face.rs` |
| Face Recognition | SFace recognizer (OpenCV) | `model/face.rs` |
| Semantic Search | CLIP-style embedding + cosine similarity | `model/aura.rs` |
| Timeline | Hiển thị theo năm/tháng | `db/operations.rs` |
| People View | Nhóm khuôn mặt theo danh tính | `db/operations.rs` |
| Albums | Nhóm thủ công | `views/albums/` |
| Duplicate Detection | SHA-256 + pHash + vector similarity | `db/operations.rs` |
| Trash & Hidden | Quản lý ảnh đã xóa/ẩn | `db/operations.rs` |
| Auto Sync | Theo dõi folder tự động (FS Watcher) | `fs_watcher.rs` |

## 1.4. Use-case chính

### Use-case 1: Import và xử lý ảnh

```
Người dùng → Chọn thư mục nguồn (First Run Modal)
           → Backend quét tất cả file ảnh/video
           → Tính SHA-256 mỗi file (dedup check)
           → Tạo record trong SurrealDB (processed = false)
           → Pipeline AI chạy tự động:
               ├── Aura Vision Model → embedding vector
               ├── YOLO → phát hiện object + mask
               └── Face Detection + Recognition
           → Cập nhật DB (processed = true, embedding, objects, faces)
           → Frontend hiển thị trong Timeline
```

### Use-case 2: Tìm kiếm bằng văn bản

```
Người dùng nhập query: "ảnh gia đình"
           → TextProcessor tokenize (PhoBERT BPE)
           → AuraModel.encode_text() → vector 512 chiều
           → SurrealDB vector::similarity::cosine search
           → Trả về danh sách media_id + score
           → Frontend hiển thị SearchResultsView
```

### Use-case 3: Tìm kiếm bằng ảnh

```
Người dùng upload ảnh query
           → Backend lưu ảnh tạm (cmd_save_search_image)
           → AuraModel.encode_image() → vector 512 chiều
           → SurrealDB cosine similarity search trên bảng embedding
           → Trả về top-K kết quả tương tự
           → Frontend hiển thị grid kết quả với similarity score
```

### Use-case 4: Nhận diện người (People Recognition)

```
Trong quá trình ingest:
  YOLO phát hiện "person" bbox
          → FaceModel.detect_from_mat(crop) → face embeddings
          → So khớp với face_db (known identities)
          → Nếu nhận ra → gán tên
          → Nếu không → tạo UUID mới (session grouping)
          → Lưu PersonDoc vào bảng person SurrealDB

Người dùng → PeopleView → nhấn đặt tên → cmd_name_person
```

### Use-case 5: FS Watcher — tự động cập nhật

```
FsWatcher giám sát thư mục nguồn (notify crate)
           → Phát hiện file mới (Create/Modify events)
           → Debounce 2000ms để gom batch
           → Kiểm tra RAM (>40% free)
           → Gọi ingest_files() pipeline
           → Emit event "ingest-progress" → Frontend refresh
```

## 1.5. Kiến trúc tổng quan

```
┌─────────────────────────────────────────────────────────────┐
│                    AURASEEK DESKTOP APP                     │
│                                                             │
│  ┌─────────────────────────────────────────────────────┐   │
│  │              REACT FRONTEND (TypeScript)             │   │
│  │                                                     │   │
│  │  App.tsx → Views → Components → AuraSeekApi         │   │
│  │  (Timeline, People, Search, Albums, Duplicates...)  │   │
│  └─────────────────────┬───────────────────────────────┘   │
│                        │  Tauri IPC (invoke/emit)           │
│  ┌─────────────────────▼───────────────────────────────┐   │
│  │              RUST BACKEND (Tauri)                    │   │
│  │                                                     │   │
│  │  main.rs (Tauri Commands) → Handlers                │   │
│  │  ├── ingest/     (image_ingest, video_ingest)       │   │
│  │  ├── model/      (aura, face, yolo)                 │   │
│  │  ├── processor/  (pipeline, vision, text)           │   │
│  │  ├── search/     (pipeline, text_search, img_search)│   │
│  │  ├── db/         (surreal, models, operations)      │   │
│  │  ├── fs_watcher  (FS change monitoring)             │   │
│  │  └── utils/      (logger, session, visualize)       │   │
│  └─────────────────────┬───────────────────────────────┘   │
│                        │                                    │
│  ┌─────────────────────▼───────────────────────────────┐   │
│  │              SURREALDB (Sidecar Process)             │   │
│  │                                                     │   │
│  │  NS: auraseek | DB: auraseek                        │   │
│  │  Tables: media, embedding, person,                  │   │
│  │          config_auraseek, search_history            │   │
│  │  Storage: SurrealKV (auraseek_data/)                │   │
│  └──────────────────────────────────────────────────── ┘   │
│                                                             │
│  ┌──────────────────────────────────────────────────────┐  │
│  │              AI MODELS (ONNX Runtime)                │  │
│  │                                                     │  │
│  │  vision_vi-sclir.onnx  → Vision Embedding (CLIP)   │  │
│  │  text_vi-sclir.onnx    → Text Embedding (PhoBERT)  │  │
│  │  yolo26n-seg.onnx      → Object Detection (YOLO)   │  │
│  │  face_detection_yunet  → Face Detector              │  │
│  │  face_recognition_sface → Face Recognizer           │  │
│  └──────────────────────────────────────────────────── ┘  │
└─────────────────────────────────────────────────────────────┘
```

---

# Chương 2: Cơ sở lý thuyết và phương pháp đề xuất

## 2.1. Tổng quan các mô hình AI

AuraSeek tích hợp **5 mô hình AI** được tổ chức trong thư mục `src-tauri/assets/models/`:

| Model file | Kích thước | Framework | Chức năng |
|-----------|-----------|-----------|-----------|
| `vision_vi-sclir.onnx` | ~download | ONNX Runtime | Vision embedding (CLIP-style) |
| `text_vi-sclir.onnx` | ~download | ONNX Runtime | Text embedding (PhoBERT-based) |
| `yolo26n-seg.onnx` | ~10.7 MB | ONNX Runtime | Object detection + segmentation |
| `face_detection_yunet_2022mar.onnx` | ~345 KB | OpenCV DNN | Face detection |
| `face_recognition_sface_2021dec.onnx` | ~36.9 MB | OpenCV DNN | Face recognition/embedding |

> **Lưu ý:** Các model `vision_vi-sclir.onnx` và `text_vi-sclir.onnx` được download tự động lần đầu chạy thông qua `cmd_download_models`, được lưu vào thư mục app data của hệ thống.

## 2.2. Mô hình YOLO — Object Detection & Segmentation

### 2.2.1. Tổng quan

**YOLO (You Only Look Once)** phiên bản Nano được tinh chỉnh cho segmentation, được lưu tại `assets/models/yolo26n-seg.onnx`. Đây là variant nhỏ gọn (Nano) của YOLO thế hệ thứ 2, phù hợp cho inferencing thời gian thực trên CPU.

**File triển khai:** `src-tauri/src/model/yolo.rs`

### 2.2.2. Kiến trúc Model

```
Input: [1, 3, 640, 640]  (batch=1, RGB, 640x640)
       ↓
YOLO Backbone + Neck
       ↓
Output 0: [1, n_det, det_dim]  → bounding boxes + confidence + class + mask coeffs
Output 1: [1, proto_c, proto_h, proto_w]  → prototype masks
```

Trong đó:
- `n_det`: số lượng proposals (thường là 8400 với input 640x640)
- `det_dim`: 4 (bbox) + 1 (conf) + 1 (class_id) + 32 (mask coefficients) = 38
- `proto_c`: 32 prototype channels
- `proto_h`, `proto_w`: kích thước prototype (thường 160x160)

### 2.2.3. Pipeline xử lý YOLO

**File:** `src-tauri/src/processor/vision/yolo_image.rs` và `yolo_postprocess.rs`

```
Ảnh gốc
  ↓ letterbox_640() — resize giữ tỷ lệ, pad về 640x640
  ↓
blob [1, 3, 640, 640]  (normalized float32)
  ↓ YoloModel.detect()
  ↓
YoloRawResult {det, protos, n_det, det_dim, proto_c/h/w}
  ↓ YoloProcessor.postprocess(conf_thresh=0.25, iou_thresh=0.45)
      ├── raw_detections() — lọc theo confidence
      ├── NMS() — Non-Maximum Suppression
      ├── process_mask() — tái tạo mask từ prototype
      │   ├── dot product coefficients × protos
      │   ├── sigmoid activation
      │   ├── bilinear resize về kích thước gốc
      │   └── binary threshold > 0.5
      └── encode_rle() — Run-Length Encoding mask
  ↓
Vec<DetectionRecord> {class_name, conf, bbox[4], mask_area, mask_rle}
```

**Triển khai NMS (Non-Maximum Suppression):**
```rust
// src-tauri/src/processor/vision/yolo_postprocess.rs
fn nms(dets: &[YoloDet], iou_thresh: f32) -> Vec<YoloDet> {
    // Sắp xếp theo confidence giảm dần
    order.sort_by(|&a, &b| dets[b].conf.partial_cmp(&dets[a].conf).unwrap());
    // Loại bỏ các box có IoU > threshold
    ...
}
```

### 2.2.4. Class Labels

YOLO model trong AuraSeek nhận diện 80 class theo chuẩn COCO dataset. Class names được nhúng trực tiếp vào file `.onnx` và được parse khi load model:

```rust
// src-tauri/src/model/yolo.rs - load_class_names()
fn load_class_names(path: &str) -> Vec<String> {
    // Parse class dict từ metadata của ONNX file
    // Format: {0: 'person', 1: 'bicycle', 2: 'car', ...}
    ...
}
```

Một số class quan trọng: `person`, `car`, `motorcycle`, `bicycle`, `truck`, `dog`, `cat`, `bird`, `laptop`, `phone`, v.v.

### 2.2.5. Mask Encoding — Run-Length Encoding (RLE)

Thay vì lưu toàn bộ bitmap mask (tốn không gian), AuraSeek sử dụng **RLE encoding**:

```rust
// Mỗi entry [offset, length] nghĩa là pixels từ offset đến offset+length đều = 1
fn encode_rle(mask: &[u8]) -> Vec<(u32, u32)> {
    // mask là flat array row-major (width × height)
    ...
}
```

Bộ sưu tập mask được lưu vào SurrealDB dưới dạng `objects.*.mask_rle: array`.

## 2.3. Mô hình Face Detection — YuNet

### 2.3.1. Tổng quan

**YuNet** là một face detector nhẹ được phát triển bởi nhóm OpenCV, lưu tại `assets/models/face_detection_yunet_2022mar.onnx` (~345 KB). YuNet nổi bật với:
- Tốc độ inference nhanh trên CPU
- Hỗ trợ CUDA khi có GPU NVIDIA
- Phát hiện face với 14 điểm đặc trưng (bbox + 5 landmarks)

**File triển khai:** `src-tauri/src/model/face.rs`

### 2.3.2. Cấu hình YuNet

```rust
// src-tauri/src/model/face.rs
const SCORE_THRESHOLD: f32 = 0.93;   // Ngưỡng confidence tối thiểu
const NMS_THRESHOLD: f32   = 0.3;    // IoU threshold cho NMS
const TOP_K: i32           = 500000; // Số proposals tối đa
```

```rust
// Tự động chọn GPU nếu có
let (backend, target, provider_name) = if opencv::core::get_cuda_enabled_device_count()? > 0 {
    (DNN_BACKEND_CUDA, DNN_TARGET_CUDA, "CUDA")
} else {
    (DNN_BACKEND_OPENCV, DNN_TARGET_CPU, "CPU")
};
```

### 2.3.3. Pipeline phát hiện khuôn mặt

```
Frame ảnh (Mat)
  ↓ resize về 320x320 (detector input size cố định)
  ↓ FaceDetectorYN.detect()
  ↓
faces_mat [n_faces, 15]:
    [:, 0:4]  = bbox (x, y, w, h)
    [:, 4:14] = 5 landmarks (mắt, mũi, miệng) × (x,y)
    [:, 14]   = confidence score
  ↓ Lọc score < 0.93
  ↓ Sort theo score giảm dần
  ↓ NMS với IoU > 0.3
  ↓ Map bbox về coordinate gốc (tỷ lệ ratio_x, ratio_y)
  ↓
Vec<FaceGroup> {face_id, name, conf, bbox[4], embedding}
```

### 2.3.4. Tích hợp với YOLO

Để tăng độ chính xác và tránh nhận diện vật thể không phải người, AuraSeek kết hợp YOLO + YuNet:

```rust
// src-tauri/src/processor/pipeline.rs
// Chỉ chạy face detection trong vùng person bbox từ YOLO
let person_bboxes: Vec<[f32; 4]> = objects.iter()
    .filter(|o| o.class_name == "person")
    .map(|o| o.bbox)
    .collect();

if person_bboxes.is_empty() {
    // Fallback: chạy trên toàn ảnh nếu YOLO không thấy người
    fm.detect_from_path(img_path, &self.face_db)
} else {
    // Crop từng person bbox và chạy face detection
    for bbox in &person_bboxes {
        let roi = Mat::roi(&frame, Rect::new(x1, y1, cw, ch))?;
        fm.detect_from_mat(&crop, &self.face_db)
    }
}
```

## 2.4. Mô hình Face Recognition — SFace

### 2.4.1. Tổng quan

**SFace** là mô hình nhận diện khuôn mặt do Zhong et al. phát triển (2021), được OpenCV tích hợp sẵn. File: `assets/models/face_recognition_sface_2021dec.onnx` (~36.9 MB).

SFace tạo ra embedding vector 128 chiều đặc trưng cho mỗi khuôn mặt, được chuẩn hoá về unit norm (L2 normalization).

### 2.4.2. Pipeline nhận diện khuôn mặt

```
Bounding box khuôn mặt từ YuNet
  ↓ FaceRecognizerSF.align_crop()
    → Crop và align khuôn mặt về 112×112 pixel
    → Dựa trên 5 landmarks (geometric transformation)
  ↓ FaceRecognizerSF.feature()
    → Inference ONNX → raw feature vector
  ↓ mat_to_vec_f32() + L2 normalization
    → embedding Vec<f32> (128 chiều, unit norm)
  ↓ FaceDb.query_id(embedding, threshold=0.33)
    → cosine_similarity với tất cả embeddings trong face_db
    → Nếu max_score > 0.33 → trả về (name, face_id)
    → Nếu không match → "unknown_placeholder"
```

**L2 Normalization:**
```rust
// src-tauri/src/model/face.rs
pub fn mat_to_vec_f32(m: &Mat) -> Result<Vec<f32>> {
    let mut v = Vec::with_capacity(m.cols() as usize);
    for j in 0..m.cols() {
        v.push(*m.at_2d::<f32>(0, j)?);
    }
    // Normalize về unit vector
    let norm = v.iter().map(|x| x * x).sum::<f32>().sqrt().max(1e-8);
    Ok(v.into_iter().map(|x| x / norm).collect())
}
```

### 2.4.3. Cosine Similarity Threshold

```rust
// src-tauri/src/model/face.rs
pub const COSINE_THRESHOLD: f32 = 0.33;
```

Ngưỡng này được dùng cho cả:
1. So khớp với `face_db` (ảnh tham chiếu đã biết danh tính)
2. Gom nhóm các khuôn mặt "unknown" trong cùng session (`session_faces`)

### 2.4.4. Face Database (FaceDb)

**File:** `src-tauri/src/processor/vision/face_image.rs`

```
assets/face_db/
  ├── Alice/          ← thư mục mang tên người
  │   ├── photo1.jpg
  │   └── photo2.jpg
  └── Bob/
      └── photo1.jpg
```

Khi khởi động, `FaceDb::build()` quét thư mục `assets/face_db/`, chạy face detection trên từng ảnh tham chiếu, lưu embedding vào memory. UUID ổn định được tạo bằng `Uuid::new_v5()` (deterministic dựa trên tên).

## 2.5. Mô hình Aura — Semantic Embedding (CLIP-style)

### 2.5.1. Tổng quan

**AuraModel** là trái tim của tính năng tìm kiếm ngữ nghĩa, triển khai kiến trúc tương tự **CLIP (Contrastive Language-Image Pretraining)** được fine-tune cho tiếng Việt.

**File:** `src-tauri/src/model/aura.rs`

Model gồm 2 nhánh:
- **Vision Tower** (`vision_vi-sclir.onnx`): encode ảnh → vector embedding
- **Text Tower** (`text_vi-sclir.onnx`): encode văn bản → vector embedding (**cùng không gian vector**)

Hai vector từ ảnh và text được học để có **cosine similarity cao** nếu ảnh và mô tả khớp nhau.

### 2.5.2. Vision Embedding Pipeline

**File:** `src-tauri/src/processor/vision/aura_image.rs`

```rust
pub fn preprocess_aura(path: &str) -> Result<Vec<f32>> {
    let img     = ImageReader::open(path)?.decode()?;
    let resized = img.resize_exact(256, 256, FilterType::Triangle);
    let rgb     = resized.to_rgb8();

    let area   = 256 * 256;
    let mut blob = vec![0.0f32; 3 * area];

    for (x, y, pixel) in rgb.enumerate_pixels() {
        let idx = (y as usize * 256) + x as usize;
        // ImageNet normalization: mean=[0.485, 0.456, 0.406], std=[0.229, 0.224, 0.225]
        blob[idx]            = (pixel[0] as f32 / 255.0 - 0.485) / 0.229; // R
        blob[idx + area]     = (pixel[1] as f32 / 255.0 - 0.456) / 0.224; // G
        blob[idx + 2 * area] = (pixel[2] as f32 / 255.0 - 0.406) / 0.225; // B
    }
    Ok(blob)
}
```

```
Ảnh gốc (bất kỳ kích thước)
  ↓ resize về 256×256 (Triangle interpolation)
  ↓ to_rgb8() — đảm bảo RGB
  ↓ CHW layout (Channel-Height-Width) với ImageNet normalization
  ↓ AuraModel.encode_image(blob, w=256, h=256)
    → ONNX input: [1, 3, 256, 256]
    → Vision session.run()
    → Output: [1, embedding_dim]
  ↓ Vec<f32> embedding vector
```

### 2.5.3. Text Embedding Pipeline

**File:** `src-tauri/src/search/text_search.rs`, `src-tauri/src/processor/text/tokenizer.rs`

```
Query text (tiếng Việt)
  ↓ normalize_text():
      - lowercase
      - split_whitespace().join(" ")
  ↓ PhobertTokenizer.tokenize():
      - Tách từ bằng regex \S+
      - BPE encoding (Byte-Pair Encoding)
      - Thêm @@ suffix cho subword units
  ↓ convert_tokens_to_ids():
      - Tra cứu vocab map (vocab.txt)
      - Unknown token = 3 (<unk>)
  ↓ build_inputs_with_special_tokens():
      - Thêm <s> (BOS) đầu, </s> (EOS) cuối
  ↓ Padding đến max_len=64
      - input_ids: padded với pad_token_id=1
      - attention_mask: 1 cho real tokens, 0 cho padding
  ↓ AuraModel.encode_text(input_ids, attention_mask)
    → ONNX inputs: "input_ids", "attention_mask"
    → Text session.run()
    → Output: [1, embedding_dim]
  ↓ Vec<f32> embedding vector
```

### 2.5.4. PhoBERT Tokenizer

**File:** `src-tauri/src/processor/text/tokenizer.rs`

PhoBERT là mô hình ngôn ngữ Transformer được pre-train trên dữ liệu tiếng Việt lớn. Tokenizer sử dụng **BPE (Byte-Pair Encoding)** với:

- `assets/tokenizer/vocab.txt`: Từ điển ~64K tokens (từ/subword)
- `assets/tokenizer/bpe.codes`: Merge rules cho BPE (~64K rules)

**Special tokens:**
```
<s>    = BOS (Beginning of Sentence), id=0
<pad>  = Padding, id=1  
</s>   = EOS (End of Sentence), id=2
<unk>  = Unknown token, id=3
```

**BPE Algorithm:**
```rust
pub fn bpe(&mut self, token: &str) -> String {
    // 1. Tách token thành chars, thêm </w> vào cuối
    // 2. Loop:
    //    a. Tìm bigram (pair) có rank thấp nhất trong bpe_ranks
    //    b. Merge bigram đó
    //    c. Lặp cho đến khi không còn bigram nào trong bpe_ranks
    // 3. Join bằng "@@ " delimiter
}
```

### 2.5.5. Cosine Similarity — Hàm Tương Đồng

```rust
// src-tauri/src/model/aura.rs
pub fn cosine_similarity(v1: &[f32], v2: &[f32]) -> f32 {
    let dot:   f32 = v1.iter().zip(v2.iter()).map(|(a, b)| a * b).sum();
    let norm1: f32 = v1.iter().map(|x| x * x).sum::<f32>().sqrt();
    let norm2: f32 = v2.iter().map(|x| x * x).sum::<f32>().sqrt();
    dot / (norm1 * norm2 + 1e-8)  // +1e-8 để tránh division by zero
}
```

Giá trị cosine similarity:
- **1.0**: Hai vector giống hệt nhau (cùng hướng)
- **0.0**: Hai vector vuông góc (không liên quan)
- **-1.0**: Hai vector ngược chiều

**Threshold mặc định:** `DEFAULT_THRESHOLD = 0.6` (trong `search/pipeline.rs`)

## 2.6. ONNX Runtime (ORT)

AuraSeek sử dụng **ORT (ONNX Runtime) v2.0.0-rc.9** (`ort = "2.0.0-rc.9"`) để chạy model vision và text embedding.

```rust
// src-tauri/src/utils/mod.rs  
pub fn build_session(path: &str) -> Result<Session> {
    // Khởi tạo ONNX session từ file .onnx
    // ORT tự động chọn execution provider phù hợp (CPU/GPU)
}
```

ORT inference được gọi qua:
```rust
let outputs = self.vision_session.run(ort::inputs!["images" => img_tensor])?;
let (_, data) = outputs[0].try_extract_tensor::<f32>()?;
```

---

# Chương 3: Dữ liệu để huấn luyện và tinh chỉnh

## 3.1. Tổng quan dữ liệu trong AuraSeek

AuraSeek không thực hiện huấn luyện model trong quá trình chạy (inference-only). Tuy nhiên, hệ thống sử dụng và tạo ra nhiều loại dữ liệu khác nhau:

| Loại dữ liệu | Nguồn gốc | Vai trò |
|---|---|---|
| Ảnh/Video người dùng | Local filesystem | Input cho pipeline AI |
| Face reference images | `assets/face_db/` | Cơ sở để nhận diện danh tính |
| Vision embeddings | Generated bởi AuraModel | Dùng cho semantic search |
| Object metadata | Generated bởi YOLO | Lưu trong DB, dùng cho filter |
| Face embeddings | Generated bởi SFace | Dùng cho people grouping |
| BPE vocab & codes | `assets/tokenizer/` | Tokenize text query |

## 3.2. Dữ liệu đầu vào — Ảnh và Video

### 3.2.1. Định dạng hỗ trợ

```rust
// src-tauri/src/ingest/image_ingest.rs
pub const IMAGE_EXTENSIONS: &[&str] = &[
    "jpg", "jpeg", "png", "bmp", "webp", "tiff", "tif", "heic", "avif"
];
pub const VIDEO_EXTENSIONS: &[&str] = &[
    "mp4", "mov", "avi", "mkv", "webm", "m4v", "flv", "wmv"
];
```

### 3.2.2. Quy trình thu thập dữ liệu

**Cách 1 — Folder Scan:**
```
Người dùng chọn thư mục nguồn
  → collect_files_recursive() quét đệ quy
  → Lọc theo IMAGE_EXTENSIONS / VIDEO_EXTENSIONS
  → Loại trừ: *.thumb.jpg, *.debug.jpg (thumbnail cache)
  → Sắp xếp alphabetically
```

**Cách 2 — Drag & Drop:**
```javascript
// src/App.tsx
const handleDrop = (e: DragEvent) => {
    const files = Array.from(e.dataTransfer?.files ?? []);
    processFiles(files);  // → AuraSeekApi.ingestFiles(paths)
}
```

**Cách 3 — Clipboard Paste:**
```javascript
const handlePaste = (e: ClipboardEvent) => {
    const files = Array.from(e.clipboardData?.files ?? []);
    // Nếu có path → ingestFiles; nếu blob → ingestImageData (base64)
}
```

### 3.2.3. Deduplication — Chống trùng lặp

AuraSeek tính **SHA-256** của mỗi file để phát hiện file trùng:

```rust
// src-tauri/src/ingest/image_ingest.rs
fn compute_sha256(path: &Path) -> Result<String> {
    let data = std::fs::read(path)?;
    let hash = Sha256::digest(&data);
    Ok(hex::encode(hash))
}
```

Logic dedup theo tên file:
```rust
// check_exact_file(): mỗi file vật lý (1 đường dẫn) chỉ ingest 1 lần
// Nhưng vẫn cho phép copy sang tên khác → được coi là media mới
pub async fn check_exact_file(db, name, sha256) -> Result<Option<(String, bool)>> {
    // Query: SELECT id, processed FROM media WHERE file.name = $name
    // processed=true → skip; processed=false → reuse media_id
}
```

## 3.3. Face Reference Database

### 3.3.1. Cấu trúc thư mục

```
assets/face_db/
  ├── [Tên người 1]/          ← Tên thư mục = tên nhận diện
  │   ├── reference1.jpg      ← Ảnh chân dung tham chiếu
  │   ├── reference2.jpg
  │   └── reference3.jpg
  ├── [Tên người 2]/
  │   └── portrait.png
  └── ...
```

Mỗi subthư mục tương ứng với một **identity**. Càng nhiều ảnh tham chiếu, độ chính xác nhận diện càng cao.

### 3.3.2. Xây dựng Face Database

```rust
// src-tauri/src/processor/vision/face_image.rs
pub fn build(db_path: &str, face_model: &mut FaceModel) -> Result<Self> {
    for identity_dir in read_dir(db_path) {
        let identity = dir_name;  // tên người
        
        // UUID v5 dựa trên tên → ổn định qua các session
        let stable_id = Uuid::new_v5(&Uuid::NAMESPACE_DNS, identity.as_bytes());
        
        for img in identity_dir {
            // Chạy face detection + recognition trên ảnh tham chiếu
            let features = face_model.extract_feature_for_db(img_path)?;
            person_embs.extend(features);
        }
        
        embeddings.insert(identity, person_embs);
    }
}
```

### 3.3.3. Session Face Grouping

Khi hệ thống gặp khuôn mặt không có trong face_db, nó sử dụng **session clustering**:

```rust
// src-tauri/src/processor/pipeline.rs
pub session_faces: Vec<(Vec<f32>, String)>  // (embedding, uuid)

// Với mỗi "unknown" face:
for f in faces.iter_mut() {
    if f.face_id == "unknown_placeholder" {
        // So sánh với tất cả face đã thấy trong session
        for (cached_emb, id) in &self.session_faces {
            let score = cosine_similarity(&f.embedding, cached_emb);
            if score > COSINE_THRESHOLD { cached_id = Some(id); }
        }
        if let Some(id) = cached_id {
            f.face_id = id;  // Gom vào nhóm đã có
        } else {
            let new_id = Uuid::new_v4().to_string();
            self.session_faces.push((f.embedding.clone(), new_id));
        }
    }
}
```

## 3.4. Vector Embedding Storage

### 3.4.1. Bảng embedding trong SurrealDB

Mỗi ảnh được lưu **1 embedding vector** (với video: nhiều vector cho từng frame đại diện):

```sql
-- Schema định nghĩa trong surreal.rs
DEFINE TABLE IF NOT EXISTS embedding SCHEMAFULL;
DEFINE FIELD IF NOT EXISTS media_id   ON embedding TYPE record<media>;
DEFINE FIELD IF NOT EXISTS source     ON embedding TYPE string;     -- "image" | "video_frame"
DEFINE FIELD IF NOT EXISTS frame_ts   ON embedding TYPE option<float>;   -- timestamp (video only)
DEFINE FIELD IF NOT EXISTS frame_idx  ON embedding TYPE option<int>;
DEFINE FIELD IF NOT EXISTS vec        ON embedding TYPE array<float>;    -- embedding vector
DEFINE FIELD IF NOT EXISTS created_at ON embedding TYPE datetime DEFAULT time::now();
```

### 3.4.2. Vector Dimension

- **Vision embedding** (AuraModel): kích thước phụ thuộc vào `vision_vi-sclir.onnx`
- **Text embedding** (AuraModel): cùng không gian vector với vision
- **Face embedding** (SFace): 128 chiều, L2-normalized

## 3.5. Cấu trúc thư mục dữ liệu `auraseek_data/`

```
auraseek_data/
  ├── LOCK               ← File khóa SurrealKV
  ├── manifest/          ← SurrealKV manifest files
  ├── sstables/          ← SurrealKV SSTable files (dữ liệu chính)
  │   └── ...
  ├── vlog/              ← Value Log (dữ liệu lớn — embedding vectors)
  │   └── ...
  └── wal/               ← Write-Ahead Log (đảm bảo tính nhất quán)
      └── ...
```

### 3.5.1. SurrealKV — Storage Engine

AuraSeek sử dụng **SurrealKV** (SurrealDB v3 default storage) — một key-value store nhúng được thiết kế riêng cho SurrealDB:

- **SSTable (Sorted String Table):** Lưu trữ chính, data được sort theo key, immutable
- **Value Log (vlog):** Tách biệt lưu trữ value lớn (như embedding vectors ~2KB mỗi vector)
- **WAL (Write-Ahead Log):** Ghi log trước khi commit vào SSTable, đảm bảo durability
- **Manifest:** Metadata về các SSTable files, compaction state

### 3.5.2. SurrealDB Sidecar

SurrealDB chạy như một **child process** của AuraSeek:

```rust
// src-tauri/src/surreal_sidecar.rs
pub fn start_surreal(binary, data_dir, port, user, pass) -> Result<Child> {
    let db_uri = format!("surrealkv://{}", data_dir.join("auraseek.db").display());
    Command::new(binary)
        .args(["start", "--bind", &bind_addr, "--user", user, "--pass", pass, "--log", "warn", &db_uri])
        .spawn()
}
```

Strategy kết nối:
1. Tìm SurrealDB đang chạy sẵn trong range port 8000-9000 (reuse từ session trước)
2. Nếu không có → tìm free port → khởi động process mới → đợi ready

## 3.6. Tokenizer Data

```
assets/tokenizer/
  ├── vocab.txt      ← ~895 KB — Từ điển PhoBERT (~64K tokens)
  └── bpe.codes      ← ~1.1 MB — BPE merge rules
```

**vocab.txt format:**
```
<s> 0
<pad> 1
</s> 2
<unk> 3
mẫu@@ 4
...
```

**bpe.codes format:**
```
e n</w>    ← merge rule: "e" + "n</w>" → "en</w>"
i ng</w>
t h</w>
...
```

---

# Chương 4: Đánh giá hiệu suất của mô hình

## 4.1. Hiệu suất Inference

### 4.1.1. Benchmark Tốc độ xử lý

Dựa trên log thực tế của hệ thống (từ `image_ingest.rs`):

```
✅ Done in Xms | objects=N faces=M embed_dims=D
```

**Ước tính thời gian xử lý 1 ảnh (CPU, không GPU):**

| Bước | Model | Thời gian ước tính |
|------|-------|-------------------|
| Preprocess (resize, normalize) | - | ~5-10ms |
| YOLO detection + segmentation | yolo26n-seg (10.7MB) | ~50-150ms |
| Face detection (YuNet) | face_detection_yunet (345KB) | ~10-30ms |
| Face recognition (SFace) | face_recognition_sface (36.9MB) | ~20-60ms per face |
| Vision embedding | vision_vi-sclir | ~30-100ms |
| DB write (SurrealDB) | - | ~5-20ms |
| **Tổng** | | **~120-370ms** |

> **Lưu ý:** Thời gian thực tế phụ thuộc lớn vào: CPU model, số core, cache warmup (lần đầu chậm hơn do model load), kích thước ảnh, số khuôn mặt/object trong ảnh.

### 4.1.2. Tốc độ Video Processing

Với video, pipeline phức tạp hơn:

```rust
// src-tauri/src/ingest/video_ingest.rs
// 1. ffprobe: lấy fps + total frames (~200ms)
// 2. Scene detection bằng ffmpeg (~depends on video length)
// 3. Extract frames: 3 frames/scene (~100ms/frame)
// 4. AI processing mỗi frame (~120-370ms)
// 5. Thumbnail generation
```

**Công thức ước tính:**
```
T_video ≈ T_probe + T_scenes + N_scenes × 3 × T_frame_extract
        + N_frames × T_ai + T_thumb
```

Với video 5 phút, 5 scenes: ≈ 15 frames AI = **~2-6 giây tổng**

## 4.2. Độ chính xác Detection

### 4.2.1. YOLO Object Detection

Model `yolo26n-seg.onnx` là **YOLOv8n-seg** — phiên bản Nano:

| Metric | YOLOv8n-seg (COCO) |
|--------|-------------------|
| mAP50 | ~36.7% |
| mAP50-95 | ~21.3% |
| Params | ~3.4M |
| Size | ~10.7 MB (ONNX) |

**Confidence threshold:** `0.25` (loại bỏ detection yếu)
**IoU threshold:** `0.45` (NMS)

### 4.2.2. YuNet Face Detection

| Metric | YuNet 2022mar |
|--------|--------------|
| Score threshold | 0.93 (cao — giảm false positive) |
| NMS threshold | 0.3 |
| Min face size | 20×20 px |
| Detector input | Fixed 320×320 |
| Speed (CPU) | ~10-30ms per image |

### 4.2.3. SFace Face Recognition

| Metric | SFace 2021dec |
|--------|--------------|
| Embedding dim | 128 |
| Cosine threshold | 0.33 |
| LFW accuracy | ~99.6% |
| IJB-C (TAR@FAR=1e-4) | ~93.3% |

**Lưu ý về threshold 0.33:**
- Threshold thấp → nhạy, dễ match (nhiều false positive)
- Threshold cao → nghiêm, khó match (nhiều false negative)
- 0.33 là giá trị cân bằng cho môi trường thực tế

## 4.3. Hiệu suất Tìm kiếm Vector

### 4.3.1. SurrealDB Vector Search

AuraSeek sử dụng `vector::similarity::cosine` built-in của SurrealDB:

```sql
-- src-tauri/src/db/operations.rs
SELECT
    media_id,
    vector::similarity::cosine(vec, $qvec) AS score
FROM embedding
WHERE vector::similarity::cosine(vec, $qvec) >= $thresh
ORDER BY score DESC
LIMIT $lim
```

**Default parameters:**
```rust
const DEFAULT_THRESHOLD: f32 = 0.6;    // search/pipeline.rs
const DEFAULT_LIMIT: usize = 10000;
```

### 4.3.2. Scaling và Hiệu suất

| Số lượng ảnh | Số embedding | Thời gian search ước tính |
|-----------|-------------|--------------------------|
| 1,000 ảnh | ~1,000 vecs | < 100ms |
| 10,000 ảnh | ~10,000 vecs | < 500ms |
| 100,000 ảnh | ~100,000 vecs | 1-5s |
| 1,000,000 ảnh | ~1M vecs | >10s (cần index) |

> **Giới hạn hiện tại:** SurrealDB thực hiện **full scan** (O(n)) cho vector search — không có ANN index (Approximate Nearest Neighbor). Điều này ổn với bộ sưu tập < 50,000 ảnh nhưng sẽ chậm với dataset lớn hơn.

## 4.4. Các yếu tố ảnh hưởng đến hiệu suất

### 4.4.1. CPU vs GPU

```rust
// src-tauri/src/model/face.rs
// Tự động detect CUDA GPU
let (backend, target) = if opencv::core::get_cuda_enabled_device_count()? > 0 {
    (DNN_BACKEND_CUDA, DNN_TARGET_CUDA)  // NVIDIA GPU
} else {
    (DNN_BACKEND_OPENCV, DNN_TARGET_CPU)  // Fallback CPU
};
```

Với **NVIDIA GPU + CUDA**:
- Face detection: nhanh hơn ~5-10x
- Face recognition: nhanh hơn ~3-5x

**ONNX Runtime (AuraModel, YOLO):** Hiện tại dùng CPU provider. Có thể thêm CUDA execution provider cho ORT để tăng tốc thêm.

### 4.4.2. RAM

```rust
// src-tauri/src/fs_watcher.rs
let ram_pct = crate::available_ram_percent();
if ram_pct < 40.0 {
    // Bỏ qua batch nếu RAM < 40% free
    log_warn!("not enough RAM ({:.1}%), skipping batch", ram_pct);
}
```

Model memory footprint:
- AuraModel vision: ~tùy model size
- AuraModel text: ~tùy model size
- YOLO: ~50-100MB RAM khi loaded
- SFace: ~150-300MB RAM khi loaded
- YuNet: ~10-20MB RAM khi loaded

### 4.4.3. Kích thước ảnh

- Ảnh lớn (4K) → preprocess lâu hơn (resize về 256/640)
- YOLO/Face detection: luôn resize về fixed size (640/320) trước inference
- Ảnh nhỏ (<20px) bị bỏ qua trong face detection

### 4.4.4. Số lượng đối tượng trong ảnh

- Nhiều người trong ảnh → nhiều face detection crop → slower
- Nhiều object class → nhiều NMS computation


---

# Chương 5: Thiết kế phần mềm và Triển khai kỹ thuật

## 5.1. Kiến trúc tổng thể

AuraSeek được xây dựng theo mô hình **Desktop Application** với kiến trúc phân lớp rõ ràng:

```
┌──────────────────────────────────────────────────────────────┐
│  LAYER 1: PRESENTATION LAYER (React Frontend)                │
│                                                              │
│  App.tsx                                                     │
│  ├── Sidebar Navigation                                      │
│  ├── Topbar (Search + Filters)                               │
│  └── Views:                                                  │
│      ├── TimelineView      ← Hiển thị ảnh theo timeline     │
│      ├── SearchResultsView ← Kết quả tìm kiếm               │
│      ├── PeopleView        ← Nhóm khuôn mặt                 │
│      ├── AlbumsView        ← Album                          │
│      ├── DuplicatesView    ← Ảnh trùng lặp                  │
│      ├── TrashView         ← Thùng rác                      │
│      └── HiddenView        ← Ảnh ẩn                         │
├──────────────────────────────────────────────────────────────┤
│  LAYER 2: BRIDGE LAYER (Tauri IPC)                          │
│                                                              │
│  src/lib/api.ts → AuraSeekApi → invoke("cmd_*")             │
│  Tauri Events: "ingest-progress", "model-download-progress"  │
├──────────────────────────────────────────────────────────────┤
│  LAYER 3: APPLICATION LAYER (Rust - main.rs)                │
│                                                              │
│  #[tauri::command] handlers                                  │
│  ├── cmd_init()          ← Khởi động AI engine + DB         │
│  ├── cmd_scan_folder()   ← Import folder                    │
│  ├── cmd_search_text()   ← Text search                      │
│  ├── cmd_search_image()  ← Image search                     │
│  ├── cmd_get_timeline()  ← Lấy timeline                     │
│  ├── cmd_get_people()    ← Lấy danh sách người              │
│  └── cmd_get_duplicates()← Phát hiện trùng                  │
├──────────────────────────────────────────────────────────────┤
│  LAYER 4: DOMAIN LAYER (Rust modules)                       │
│                                                              │
│  ├── ingest/   : Pipeline nhập ảnh/video                    │
│  ├── model/    : AI model wrappers                          │
│  ├── processor/: Pre/post processing                        │
│  ├── search/   : Search orchestration                       │
│  └── db/       : Database abstraction                       │
├──────────────────────────────────────────────────────────────┤
│  LAYER 5: INFRASTRUCTURE LAYER                              │
│                                                              │
│  ├── SurrealDB (sidecar process, SurrealKV storage)         │
│  ├── ONNX Runtime (vision/text models)                      │
│  ├── OpenCV DNN (face detection/recognition)                │
│  ├── ffmpeg/ffprobe (video processing)                      │
│  └── Axum HTTP server (video streaming)                     │
└──────────────────────────────────────────────────────────────┘
```

## 5.2. Cấu trúc Source Code

### 5.2.1. Cấu trúc thư mục tổng thể

```
auraseek/
├── src/                         ← React Frontend (TypeScript)
│   ├── App.tsx                  ← Root component, routing
│   ├── main.tsx                 ← Entry point
│   ├── index.css                ← Global styles
│   ├── components/              ← UI Components
│   │   ├── common/              ← Shared components
│   │   ├── layout/              ← AppSidebar, AppTopbar
│   │   ├── photo-detail/        ← Chi tiết ảnh
│   │   ├── photos/              ← Photo grid, card
│   │   ├── ui/                  ← shadcn/ui base components
│   │   └── video/               ← Video player
│   ├── contexts/                ← React Context (SelectionContext)
│   ├── hooks/                   ← Custom React hooks
│   ├── lib/                     ← Utilities
│   │   ├── api.ts               ← Tauri API wrapper
│   │   └── utils.ts             ← Helper functions
│   ├── types/                   ← TypeScript types
│   │   └── photo.type.ts        ← Photo/media type definitions
│   └── views/                   ← Page-level views
│       ├── albums/              ← AlbumsView
│       ├── duplicates/          ← DuplicatesView
│       ├── gallery/             ← FilteredGalleryView
│       ├── hidden/              ← HiddenView
│       ├── people/              ← PeopleView
│       ├── photos/              ← PhotosView
│       ├── search/              ← SearchResultsView
│       ├── timeline/            ← TimelineView
│       └── trash/               ← TrashView
│
├── src-tauri/                   ← Rust Backend (Tauri)
│   ├── Cargo.toml               ← Dependencies
│   ├── build.rs                 ← Build script (download models)
│   ├── tauri.conf.json          ← Tauri config
│   ├── assets/                  ← Static assets
│   │   ├── models/              ← ONNX model files
│   │   ├── tokenizer/           ← BPE vocab + codes
│   │   └── face_db/             ← Face reference images
│   └── src/
│       ├── main.rs              ← Tauri commands, app lifecycle
│       ├── fs_watcher.rs        ← File system monitoring
│       ├── surreal_sidecar.rs   ← SurrealDB process management
│       ├── downloader.rs        ← Model download
│       ├── debug_cli.rs         ← Debug/CLI tools
│       ├── db/                  ← Database layer
│       │   ├── mod.rs
│       │   ├── surreal.rs       ← Connection management
│       │   ├── models.rs        ← Data models/structs
│       │   └── operations.rs    ← CRUD + vector search
│       ├── model/               ← AI model wrappers
│       │   ├── mod.rs
│       │   ├── aura.rs          ← CLIP-style embedding model
│       │   ├── face.rs          ← YuNet + SFace
│       │   └── yolo.rs          ← YOLO segmentation
│       ├── processor/           ← AI processing
│       │   ├── mod.rs
│       │   ├── pipeline.rs      ← AuraSeekEngine (main AI orchestrator)
│       │   ├── text/            ← Text processing
│       │   │   ├── mod.rs       ← TextProcessor
│       │   │   └── tokenizer.rs ← PhoBERT BPE tokenizer
│       │   └── vision/          ← Vision processing
│       │       ├── mod.rs
│       │       ├── aura_image.rs    ← Image preprocessing for Aura
│       │       ├── face_image.rs    ← FaceDb, cosine_similarity
│       │       ├── yolo_image.rs    ← Letterbox resize
│       │       └── yolo_postprocess.rs ← NMS, mask, RLE
│       ├── ingest/              ← Data ingestion
│       │   ├── mod.rs
│       │   ├── image_ingest.rs  ← Image pipeline
│       │   └── video_ingest.rs  ← Video pipeline
│       ├── search/              ← Search system
│       │   ├── mod.rs
│       │   ├── pipeline.rs      ← SearchPipeline orchestrator
│       │   ├── text_search.rs   ← Text → embedding → search
│       │   └── image_search.rs  ← Image → embedding → search
│       └── utils/               ← Utilities
│           ├── mod.rs
│           ├── logger.rs        ← Logging macros
│           ├── session.rs       ← ONNX session builder
│           └── visualize.rs     ← Debug visualization
│
├── auraseek_data/               ← Runtime data (SurrealDB)
│   ├── LOCK
│   ├── manifest/
│   ├── sstables/
│   ├── vlog/
│   └── wal/
│
├── package.json                 ← npm/yarn config
├── vite.config.ts               ← Vite bundler config
└── index.html                   ← HTML entry point
```

## 5.3. Frontend — React + TypeScript

### 5.3.1. Cấu trúc App.tsx

`App.tsx` là component root, quản lý toàn bộ state và routing:

```typescript
// src/App.tsx
function App() {
  // State management
  const [route, setRoute] = useState<AppRoute>({ view: "timeline" });
  const [timelineGroups, setTimelineGroups] = useState<TimelineGroup[]>([]);
  const [searchResults, setSearchResults] = useState<SearchResult[]>([]);
  const [people, setPeople] = useState<PersonGroup[]>([]);

  // Lifecycle: khởi động app
  useEffect(() => {
    initialize();  // checkModels → downloadModels → init → loadTimeline
  }, []);

  // Routing (client-side, không dùng React Router)
  const renderView = () => {
    switch (route.view) {
      case "timeline":     return <TimelineView ... />;
      case "search_results": return <SearchResultsView ... />;
      case "people":       return <PeopleView ... />;
      ...
    }
  };

  return (
    <SelectionProvider>
      <TooltipProvider>
        <SidebarProvider>
          <AppSidebar />
          <main>
            <AppTopbar />
            {renderView()}
          </main>
        </SidebarProvider>
      </TooltipProvider>
    </SelectionProvider>
  );
}
```

### 5.3.2. AuraSeekApi — Tauri IPC Bridge

**File:** `src/lib/api.ts`

Toàn bộ giao tiếp với backend Rust đi qua `invoke()` của Tauri:

```typescript
export const AuraSeekApi = {
    // Khởi tạo
    async init(): Promise<string>              { return invoke("cmd_init"); },
    async checkModels(): Promise<boolean>      { return invoke("cmd_check_models"); },
    async downloadModels(): Promise<void>      { return invoke("cmd_download_models"); },

    // Import
    async scanFolder(sourcePath: string)       { return invoke("cmd_scan_folder", {sourcePath}); },
    async ingestFiles(filePaths: string[])     { return invoke("cmd_ingest_files", {filePaths}); },
    async ingestImageData(data, ext)           { return invoke("cmd_ingest_image_data", {data, ext}); },

    // Search
    async searchText(query, filters)           { return invoke("cmd_search_text", {query, filters}); },
    async searchImage(imagePath, filters)      { return invoke("cmd_search_image", {imagePath, filters}); },
    async searchCombined(text, imagePath, f)   { return invoke("cmd_search_combined", {text, imagePath, filters:f}); },

    // Data
    async getTimeline(limit?)                  { return invoke("cmd_get_timeline", {limit}); },
    async getPeople()                          { return invoke("cmd_get_people"); },
    async getDuplicates(mediaType?)            { return invoke("cmd_get_duplicates", {mediaType}); },

    // Quản lý
    async toggleFavorite(mediaId)              { return invoke("cmd_toggle_favorite", {mediaId}); },
    async moveToTrash(mediaId)                 { return invoke("cmd_move_to_trash", {mediaId}); },
    async namePerson(faceId, name)             { return invoke("cmd_name_person", {faceId, name}); },
};
```

**URL helpers:**
```typescript
// Chuyển file path → URL hiển thị
export function localFileUrl(filePath: string): string {
    return convertFileSrc(filePath);  // asset:// protocol
}

// Cho video thumbnails (cần HTTP streaming)
export async function streamFileUrl(filePath: string): Promise<string> {
    const port = await getStreamPort();
    return `http://127.0.0.1:${port}/stream?path=${encodeURIComponent(filePath)}`;
}
```

### 5.3.3. Các Views chính

**TimelineView** (`src/views/timeline/`):
- Hiển thị ảnh/video nhóm theo năm/tháng
- Lazy loading, infinite scroll
- Hover overlay hiển thị detected objects
- Multi-selection mode

**SearchResultsView** (`src/views/search/`):
- Hiển thị kết quả search với similarity score
- Filter theo object, face, date, media type

**PeopleView** (`src/views/people/`):
- Grid các face cluster
- Click để đặt tên
- Xem tất cả ảnh có người đó

**DuplicatesView** (`src/views/duplicates/`):
- Nhóm các ảnh/video trùng lặp
- Hiển thị lý do trùng (SHA-256 / pHash / vector)
- Cho phép xóa bản sao

### 5.3.4. SelectionContext

```typescript
// src/contexts/SelectionContext.tsx
// Quản lý multi-selection state cho bulk operations
const SelectionProvider = () => {
    const [selectedIds, setSelectedIds] = useState<Set<string>>(new Set());
    // selectAll, deselectAll, toggleSelect, isSelected
};
```

## 5.4. Backend — Rust Modules

### 5.4.1. main.rs — Tauri Command Handlers

`src-tauri/src/main.rs` (42KB) — file lớn nhất, định nghĩa tất cả `#[tauri::command]`:

```rust
// App state được share qua Tauri State
struct AppState {
    db:          Arc<Mutex<Option<SurrealDb>>>,
    engine:      Arc<Mutex<Option<AuraSeekEngine>>>,
    watcher:     Mutex<Option<FsWatcherHandle>>,
    source_dir:  Mutex<String>,
    sync_status: Arc<Mutex<SyncStatus>>,
    surreal_child: Mutex<Option<Child>>,
    stream_port: Mutex<Option<u16>>,
    thumb_cache_dir: Option<PathBuf>,
}

#[tauri::command]
async fn cmd_init(state: State<'_, AppState>, app: AppHandle) -> Result<String, String> {
    // 1. Start SurrealDB sidecar
    // 2. Connect SurrealDb
    // 3. Load AI models → AuraSeekEngine
    // 4. Start Axum HTTP server (video streaming)
    // 5. Start FS Watcher
    // 6. Return "Engine + DB ready"
}
```

**Danh sách đầy đủ các commands:**

| Command | Chức năng |
|---------|-----------|
| `cmd_init` | Khởi động toàn bộ hệ thống |
| `cmd_check_models` | Kiểm tra model files tồn tại |
| `cmd_download_models` | Tải model từ internet |
| `cmd_scan_folder` | Quét và import toàn bộ folder |
| `cmd_auto_scan` | Tự động scan folder đã cấu hình |
| `cmd_ingest_files` | Import danh sách file cụ thể |
| `cmd_ingest_image_data` | Import ảnh từ base64 (clipboard) |
| `cmd_search_text` | Tìm bằng text |
| `cmd_search_image` | Tìm bằng ảnh |
| `cmd_search_combined` | Tìm kết hợp text + ảnh |
| `cmd_search_object` | Lọc theo object class |
| `cmd_search_face` | Lọc theo tên người |
| `cmd_search_filter_only` | Lọc không có query |
| `cmd_get_timeline` | Lấy timeline |
| `cmd_get_people` | Lấy danh sách người |
| `cmd_get_duplicates` | Phát hiện trùng lặp |
| `cmd_get_trash` | Lấy ảnh trong thùng rác |
| `cmd_get_hidden_photos` | Lấy ảnh ẩn |
| `cmd_toggle_favorite` | Yêu thích/bỏ yêu thích |
| `cmd_move_to_trash` | Xóa vào thùng rác |
| `cmd_restore_from_trash` | Khôi phục từ thùng rác |
| `cmd_empty_trash` | Dọn thùng rác |
| `cmd_hide_photo` | Ẩn ảnh |
| `cmd_unhide_photo` | Bỏ ẩn |
| `cmd_name_person` | Đặt tên cho người |
| `cmd_get_source_dir` | Lấy thư mục nguồn |
| `cmd_set_source_dir` | Đặt thư mục nguồn |
| `cmd_get_sync_status` | Lấy trạng thái sync |
| `cmd_get_stream_port` | Lấy port video streaming |
| `cmd_cleanup_database` | Dọn dẹp DB (file không còn tồn tại) |
| `cmd_reset_database` | Reset toàn bộ DB |
| `cmd_get_distinct_objects` | Lấy danh sách object classes |
| `cmd_get_file_size` | Lấy kích thước file |
| `cmd_save_search_image` | Lưu ảnh tạm cho search |
| `cmd_delete_file` | Xóa file tạm |
| `cmd_authenticate_os` | Xác thực OS (biometric/password) |
| `cmd_get_device_name` | Lấy tên thiết bị |

### 5.4.2. fs_watcher.rs — File System Watcher

**File:** `src-tauri/src/fs_watcher.rs`

Sử dụng `notify` crate để theo dõi thay đổi filesystem:

```
notify::RecommendedWatcher watches source_dir (RecursiveMode::Recursive)
  │
  ├── OS reports Create/Modify events
  │
  ▼
mpsc::channel<PathBuf>  (capacity: 512)
  │
  ▼
Tokio async task (debounce loop):
  ├── Nhận events → HashSet<PathBuf> (gom batch)
  ├── Debounce 2000ms (tránh process file đang copy)
  ├── Kiểm tra RAM < 40% free → skip
  ├── Cập nhật SyncStatus = "syncing"
  ├── Gọi image_ingest::ingest_files()
  └── Cập nhật SyncStatus = "done" | "error"
```

```rust
// src-tauri/src/fs_watcher.rs
const DEBOUNCE_MS: u64 = 2000;  // 2 giây debounce

pub fn start_watching(source_dir, db, engine, sync_status, thumb_cache_dir)
    -> Result<FsWatcherHandle>
{
    // Returns handle chứa _watcher và stop_tx
    // Drop handle → stop watcher
}
```

### 5.4.3. ingest/image_ingest.rs — Image Pipeline

**File:** `src-tauri/src/ingest/image_ingest.rs` (526 dòng)

```
ingest_folder(source_dir, db, engine, app, thumb_cache_dir)
  │
  ├── collect_files_recursive() → (image_files, video_files)
  │
  ├── Thread 1 (scan): Tokio spawn
  │   └── for each file:
  │       ├── compute_sha256()
  │       ├── get_image_dimensions()
  │       ├── check_exact_file() → skip if processed
  │       └── insert_media() → media_id
  │
  └── Thread 2 (AI processing): sequential
      └── for each (path, media_id, is_video):
          ├── if image: process_image_file()
          │   ├── analyze_image_raw() → EngineOutput
          │   │   └── engine.process_image()
          │   ├── update_media_ai() → objects, faces
          │   ├── insert_embedding() → vision vector
          │   └── upsert_person() → face cluster
          └── if video: video_ingest::process_video()
          └── emit "ingest-progress" event
```

**Key structs:**
```rust
pub struct IngestSummary {
    pub total_found: usize,
    pub newly_added: usize,
    pub skipped_dup: usize,
    pub errors:      usize,
}

pub struct IngestProgress {
    pub processed:    usize,
    pub total:        usize,
    pub current_file: String,
}
```

### 5.4.4. ingest/video_ingest.rs — Video Pipeline

**File:** `src-tauri/src/ingest/video_ingest.rs` (411 dòng)

```
process_video(video_path, media_id, db, engine, thumb_cache_dir)
  │
  ├── probe_video() [ffprobe]
  │   ├── fps = r_frame_rate (fraction parser)
  │   └── total_frames = nb_frames || duration*fps
  │
  ├── detect_scenes() [ffmpeg showinfo filter]
  │   ├── filter: "select='gt(scene,0.11)',showinfo"
  │   └── parse pts_time từ stderr → Vec<frame_idx>
  │
  ├── Build scene ranges: (start, end) pairs
  │
  ├── For each scene:
  │   ├── Candidate frames: [20%, 50%, 80%] của scene
  │   ├── extract_frame() [ffmpeg -ss -vframes 1]
  │   ├── is_good_brightness() [Rec.601 luma, 25-240]
  │   └── Keep best frames, delete rejected
  │
  ├── For each frame:
  │   ├── analyze_image_raw() → EngineOutput
  │   ├── Aggregate objects (max conf per class)
  │   ├── Aggregate faces (max conf per face_id)
  │   └── insert_embedding(source="video_frame", frame_ts, frame_idx)
  │
  ├── Generate thumbnail [first processed frame]
  │
  └── update_media_ai(aggregated objects + faces + thumbnail)
      upsert_person() for each face
```

### 5.4.5. processor/pipeline.rs — AI Engine

**File:** `src-tauri/src/processor/pipeline.rs` (187 dòng)

`AuraSeekEngine` là orchestrator chính:

```rust
pub struct AuraSeekEngine {
    pub aura:         AuraModel,      // CLIP-style vision+text
    pub text_proc:    TextProcessor,  // PhoBERT tokenizer
    pub yolo:         YoloModel,      // Object detection
    pub face:         Option<FaceModel>, // YuNet + SFace
    pub face_db:      FaceDb,         // Known identity embeddings
    pub session_faces: Vec<(Vec<f32>, String)>, // Unknown face grouping
}

pub fn process_image(&mut self, img_path: &str) -> Result<EngineOutput> {
    // 1. Vision embedding (Aura)
    let vision_emb = self.aura.encode_image(preprocess_aura(img_path)?, 256, 256)?;

    // 2. YOLO detection + segmentation
    let lb = letterbox_640(img_path)?;
    let raw = self.yolo.detect(lb.blob.clone())?;
    let objects = YoloProcessor::postprocess(&raw, &lb, 0.25, 0.45);

    // 3. Face detection trong person bboxes
    let faces = detect_faces_in_persons(&objects, img_path, &self.face_db);

    // 4. Session face grouping cho unknowns
    resolve_session_faces(&mut faces, &mut self.session_faces);

    Ok(EngineOutput { objects, faces, vision_embedding: vision_emb })
}
```

### 5.4.6. search/pipeline.rs — Search System

**File:** `src-tauri/src/search/pipeline.rs` (169 dòng)

```rust
pub enum SearchMode {
    Text,        // Tìm bằng text query
    Image,       // Tìm bằng ảnh
    Combined,    // Tìm bằng cả text + ảnh
    ObjectFilter,// Lọc theo object class
    FaceFilter,  // Lọc theo tên mặt
    FilterOnly,  // Lọc không có vector query
}
```

**Combined Search:**
```rust
// Chạy cả text search và image search song song
let text_hits  = search_by_text_embedding(db, &text_emb, thresh, limit).await?;
let img_hits   = search_by_image_embedding(db, &img_emb, thresh, limit).await?;

// Intersection + average score
let text_map: HashMap<String, f32> = text_hits.into_iter().collect();
for (mid, img_score) in img_hits {
    if let Some(text_score) = text_map.get(&mid) {
        combined.push((mid, (img_score + text_score) / 2.0));
    }
}
```

### 5.4.7. db/operations.rs — Database Operations

**File:** `src-tauri/src/db/operations.rs` (965 dòng)

Các operations quan trọng:

```rust
// Vector search
pub async fn vector_search(db, query_vec, threshold, limit) -> Result<Vec<(String, f32)>> {
    db.db.query("
        SELECT media_id, vector::similarity::cosine(vec, $qvec) AS score
        FROM embedding
        WHERE vector::similarity::cosine(vec, $qvec) >= $thresh
        ORDER BY score DESC LIMIT $lim
    ")
}

// Timeline query
pub async fn get_timeline(db, limit, source_dir) -> Result<Vec<TimelineGroup>> {
    db.db.query("
        SELECT * FROM media
        WHERE deleted_at = NONE AND is_hidden = false AND processed = true
        ORDER BY metadata.created_at DESC LIMIT $lim
    ")
}

// Duplicate detection (3 stages)
pub async fn get_duplicates(db, source_dir, media_type, thumb_cache_dir) -> Result<Vec<DuplicateGroup>> {
    // Stage 1: SHA-256 exact duplicates
    // Stage 2: pHash near-duplicates (Hamming <= 8)
    // Stage 3: Embedding cosine similarity >= 0.92
}
```

### 5.4.8. db/surreal.rs — Database Connection

**File:** `src-tauri/src/db/surreal.rs`

Strategy kết nối 2 lớp:
1. **WebSocket** (ws://) — latency thấp hơn cho streaming queries
2. **HTTP fallback** — đơn giản, ổn định hơn trong môi trường sandbox

```rust
pub async fn connect(addr, user, pass) -> Result<Self> {
    // Try WS với timeout 10s
    match tokio::time::timeout(10s, connect_ws(addr)).await {
        Ok(Ok(db)) => return Self::finish_connect(db, ...),
        _ => {} // fallback to HTTP
    }
    // HTTP fallback
    let db = tokio::time::timeout(10s, connect_http(addr)).await??;
    Self::finish_connect(db, ...)
}
```

### 5.4.9. db/models.rs — Data Models

**File:** `src-tauri/src/db/models.rs` (291 dòng)

Các struct chính:

```rust
pub struct MediaDoc {          // Stored trong bảng `media`
    pub media_type: String,    // "image" | "video"
    pub file:       FileInfo,  // name, size, sha256, phash
    pub metadata:   MediaMetadata, // width, height, duration, fps, created_at
    pub objects:    Vec<ObjectEntry>,  // YOLO results
    pub faces:      Vec<FaceEntry>,    // Face detection results
    pub processed:  bool,      // AI pipeline đã chạy chưa
    pub thumbnail:  Option<String>,  // Path ảnh thumbnail (video)
    pub deleted_at: Option<Datetime>,
    pub is_hidden:  bool,
}

pub struct EmbeddingDoc {      // Stored trong bảng `embedding`
    pub media_id:  RecordId,   // FK → media
    pub source:    String,     // "image" | "video_frame"
    pub frame_ts:  Option<f64>,
    pub frame_idx: Option<u32>,
    pub vec:       Vec<f32>,   // Embedding vector
}

pub struct PersonDoc {         // Stored trong bảng `person`
    pub face_id:   String,     // UUID
    pub name:      Option<String>,
    pub thumbnail: Option<String>,
    pub conf:      Option<f32>,
    pub face_bbox: Option<Bbox>,
}
```

## 5.5. Database Schema — SurrealDB

### 5.5.1. Bảng media

```sql
DEFINE TABLE media SCHEMAFULL;
DEFINE FIELD media_type          ON media TYPE string;
DEFINE FIELD file.name           ON media TYPE string;      -- filename only
DEFINE FIELD file.size           ON media TYPE int;
DEFINE FIELD file.sha256         ON media TYPE string;
DEFINE FIELD file.phash          ON media TYPE option<string>;
DEFINE FIELD metadata.width      ON media TYPE option<int>;
DEFINE FIELD metadata.height     ON media TYPE option<int>;
DEFINE FIELD metadata.duration   ON media TYPE option<float>;
DEFINE FIELD metadata.fps        ON media TYPE option<float>;
DEFINE FIELD metadata.created_at ON media TYPE option<datetime>;
DEFINE FIELD objects             ON media TYPE array DEFAULT [];
DEFINE FIELD objects.*.class_name ON media TYPE string;
DEFINE FIELD objects.*.conf       ON media TYPE float;
DEFINE FIELD objects.*.bbox.x     ON media TYPE float;
DEFINE FIELD objects.*.mask_rle   ON media TYPE option<array>;
DEFINE FIELD faces               ON media TYPE array DEFAULT [];
DEFINE FIELD faces.*.face_id     ON media TYPE string;
DEFINE FIELD faces.*.name        ON media TYPE option<string>;
DEFINE FIELD processed           ON media TYPE bool DEFAULT false;
DEFINE FIELD favorite            ON media TYPE bool DEFAULT false;
DEFINE FIELD deleted_at          ON media TYPE option<datetime>;
DEFINE FIELD is_hidden           ON media TYPE bool DEFAULT false;
DEFINE FIELD thumbnail           ON media TYPE option<string>;

DEFINE INDEX idx_sha256  ON media FIELDS file.sha256;
DEFINE INDEX idx_created ON media FIELDS metadata.created_at;
```

### 5.5.2. Bảng embedding (Vector Store)

```sql
DEFINE TABLE embedding SCHEMAFULL;
DEFINE FIELD media_id   ON embedding TYPE record<media>;
DEFINE FIELD source     ON embedding TYPE string;       -- "image" | "video_frame"
DEFINE FIELD frame_ts   ON embedding TYPE option<float>;
DEFINE FIELD frame_idx  ON embedding TYPE option<int>;
DEFINE FIELD vec        ON embedding TYPE array<float>; -- embedding vector
DEFINE FIELD created_at ON embedding TYPE datetime DEFAULT time::now();
DEFINE INDEX idx_emb_media ON embedding FIELDS media_id;
```

### 5.5.3. Bảng person (Face Cluster)

```sql
DEFINE TABLE person SCHEMAFULL;
DEFINE FIELD face_id   ON person TYPE string;           -- UUID
DEFINE FIELD name      ON person TYPE option<string>;
DEFINE FIELD thumbnail ON person TYPE option<string>;
DEFINE FIELD conf      ON person TYPE option<float>;
DEFINE FIELD face_bbox ON person TYPE option<object>;
DEFINE FIELD created_at ON person TYPE datetime DEFAULT time::now();
DEFINE INDEX idx_face_id ON person FIELDS face_id UNIQUE;
```

### 5.5.4. Bảng config_auraseek

```sql
DEFINE TABLE config_auraseek SCHEMAFULL;
DEFINE FIELD source_dir ON config_auraseek TYPE string;
DEFINE FIELD updated_at ON config_auraseek TYPE datetime DEFAULT time::now();
-- Singleton record: config_auraseek:main
```

### 5.5.5. Bảng search_history

```sql
DEFINE TABLE search_history SCHEMAFULL;
DEFINE FIELD query      ON search_history TYPE option<string>;
DEFINE FIELD image_path ON search_history TYPE option<string>;
DEFINE FIELD filters    ON search_history TYPE option<object>;
DEFINE FIELD created_at ON search_history TYPE datetime DEFAULT time::now();
```

## 5.6. Video Streaming — Axum HTTP Server

Để phục vụ video thumbnail và streaming (vì Tauri `asset://` có giới hạn với paths ngoài `source_dir`), AuraSeek khởi động một **Axum HTTP server** local:

```rust
// src-tauri/src/main.rs
let stream_port = start_axum_stream_server();
// Lắng nghe trên 127.0.0.1:<random_port>
// Endpoint: GET /stream?path=/abs/path/to/file
// Hỗ trợ Range requests (HTTP 206) cho video seeking
```

```typescript
// Frontend dùng streamFileUrl() cho video thumbnails
const url = `http://127.0.0.1:${port}/stream?path=${encodeURIComponent(filePath)}`;
```

## 5.7. System Workflow — Quy trình đầy đủ

### 5.7.1. Khởi động ứng dụng

```
App startup
  │
  ├── check_models() → models exist?
  │   └── No → ModelDownloadScreen
  │       └── download_models() + emit "model-download-progress"
  │
  ├── cmd_init():
  │   ├── ensure_surreal()  → start SurrealDB sidecar
  │   ├── SurrealDb::connect()
  │   ├── ensure_schema()   → create tables if not exist
  │   ├── AuraSeekEngine::new()
  │   │   ├── AuraModel::new(vision_path, text_path)
  │   │   ├── TextProcessor::new(vocab, bpe)
  │   │   ├── YoloModel::new(yolo_path)
  │   │   ├── FaceModel::new(yunet, sface)
  │   │   └── FaceDb::build(face_db_path, face_model)
  │   ├── start_axum_stream_server()
  │   └── start_watching() → FsWatcherHandle
  │
  ├── get_source_dir() → "" (first run) | "/path/to/photos"
  │
  ├── First run? → Show FirstRunModal → user picks folder
  │
  └── get_timeline() → display photos
```

### 5.7.2. Import ảnh mới (File Add)

```
User thêm ảnh (folder scan / drag-drop / paste)
  │
  ├── collect files → filter by extension
  │
  ├── For each file:
  │   ├── compute SHA-256
  │   ├── check_exact_file() → already in DB? → skip
  │   ├── MediaDoc { media_type, file, metadata, processed=false }
  │   └── insert_media() → media:UUID
  │
  ├── AI Processing (sequential):
  │   ├── preprocess_aura(img) → [1,3,256,256] blob
  │   ├── aura.encode_image()  → vision_embedding Vec<f32>
  │   ├── letterbox_640(img)   → [1,3,640,640] blob
  │   ├── yolo.detect()        → raw detections
  │   ├── YoloProcessor::postprocess() → Vec<DetectionRecord>
  │   ├── FaceModel::detect_from_path() (or detect_from_mat for person crops)
  │   │   ├── FaceDb::query_id() → match known identity
  │   │   └── session_faces grouping for unknowns
  │   └── EngineOutput { objects, faces, vision_embedding }
  │
  ├── DB Updates:
  │   ├── update_media_ai(objects, faces, processed=true)
  │   ├── insert_embedding(media_id, "image", vec=vision_embedding)
  │   └── upsert_person(face_id, name, thumbnail, conf, bbox)
  │
  └── emit "ingest-progress" → Frontend refreshes timeline
```

### 5.7.3. Tìm kiếm ngữ nghĩa

```
User nhập query: "trẻ em chơi đùa ngoài trời"
  │
  ├── Frontend: AuraSeekApi.searchText(query, filters)
  │
  ├── Backend cmd_search_text():
  │   ├── encode_text_query(engine, "trẻ em chơi đùa ngoài trời")
  │   │   ├── normalize: lowercase + collapse whitespace
  │   │   ├── tokenize (PhoBERT BPE): ["trẻ", "em@@", "chơi", ...]
  │   │   ├── convert to ids: [456, 789, 1234, ...]
  │   │   ├── add special tokens: [0, 456, 789, ..., 2]
  │   │   ├── pad to max_len=64, attention_mask
  │   │   └── aura.encode_text() → text_embedding Vec<f32>
  │   │
  │   └── search_by_text_embedding(db, text_embedding, thresh=0.6, limit=10000)
  │       └── vector::similarity::cosine(vec, $qvec) >= 0.6
  │           ORDER BY score DESC → Vec<(media_id, score)>
  │
  ├── resolve_search_results() → từ media_id → MediaRow → SearchResult
  │
  ├── apply_filters() → lọc thêm theo object/face/month/year/media_type
  │
  └── Frontend: SearchResultsView hiển thị results với score
```

### 5.7.4. Phát hiện trùng lặp (Duplicate Detection)

```
cmd_get_duplicates(mediaType)
  │
  ├── Stage 1: SHA-256 exact match
  │   └── SELECT sha256, count(*) FROM media GROUP BY sha256 HAVING count > 1
  │       → DuplicateGroup { reason: "Trùng Hash — giống nhau 100%" }
  │
  ├── Stage 2: pHash near-duplicate
  │   ├── Lấy tất cả media có file.phash != NONE
  │   ├── Parse hex string → u64 pHash
  │   ├── DSU (Disjoint Set Union) clustering
  │   │   → Hai ảnh cùng nhóm nếu Hamming(pHash_a, pHash_b) <= 8
  │   └── → DuplicateGroup { reason: "Ảnh gần giống nhau (pHash)" }
  │
  └── Stage 3: Vector embedding similarity
      ├── Lấy tất cả embeddings từ bảng embedding
      ├── Vector space clustering
      │   → cosine_similarity >= 0.92 → cùng nhóm
      └── → DuplicateGroup { reason: "Nội dung tương tự (AI)" }
```


---

# Chương 6: Thực nghiệm, Kiểm thử và Đánh giá hiệu suất

## 6.1. Môi trường kiểm thử

### 6.1.1. Yêu cầu hệ thống tối thiểu

| Thành phần | Tối thiểu | Khuyến nghị |
|-----------|-----------|-------------|
| OS | Linux (x86_64), Windows 10, macOS 11 | Linux Ubuntu 22.04+ |
| CPU | 4 core x86_64 | 8+ core với AVX2 |
| RAM | 4 GB | 16 GB |
| Storage | 2 GB (app + models) | SSD 10+ GB |
| GPU | Không bắt buộc | NVIDIA CUDA (face detection speed) |
| ffmpeg | >= 4.0 | ffmpeg 6.x |
| SurrealDB | Bundled (sidecar) | — |

### 6.1.2. Dependencies

```toml
# src-tauri/Cargo.toml
[dependencies]
tauri        = "2"            # Desktop framework
ort          = "2.0.0-rc.9"  # ONNX Runtime
opencv       = "0.92"        # Computer vision
surrealdb    = "3.0.2"       # Database
tokio        = "1"           # Async runtime
axum         = "0.7"         # HTTP server (video streaming)
notify       = "7"            # FS watching
sha2         = "0.10"        # SHA-256
image        = "0.24"        # Image processing
serde        = "1.0"         # Serialization
reqwest      = "0.12"        # Model download
sysinfo      = "0.33"        # RAM monitoring
```

```json
// package.json (Frontend)
{
  "dependencies": {
    "@tauri-apps/api": "^2",
    "react": "^18",
    "react-dom": "^18",
    "typescript": "^5"
  }
}
```

## 6.2. Test Pipeline — Ingest

### 6.2.1. Test Ingest đơn lẻ

Khi thêm 1 ảnh mới vào folder nguồn:

**Kỳ vọng:**
1. FsWatcher phát hiện file mới trong vòng < 2200ms (debounce 2s)
2. SHA-256 được tính, file không trùng → media record được tạo
3. AI pipeline chạy trong < 500ms (ảnh nhỏ, ít object, CPU)
4. `ingest-progress` event được emit → Timeline refresh
5. Ảnh xuất hiện trong Timeline sau khi processed = true

**Log mẫu:**
```
[INFO] 👁️  FS watcher detected 1 new file(s), ingesting...
[INFO] 🤖 [AI 1/1] Processing: photo.jpg
[INFO]   ✅ Done in 287ms | objects=3 faces=1 embed_dims=512
[INFO] ✅ Ingest complete: 1 new, 0 skipped, 0 errors, 1 AI processed
```

### 6.2.2. Test Ingest folder lớn

Ví dụ import 500 ảnh cùng lúc:

**Luồng hoạt động:**
1. Thread scan: tạo 500 media stubs → gửi vào channel (capacity 64, backpressure)
2. Thread AI: nhận từng item, xử lý tuần tự
3. Progress: mỗi file emit event → Frontend cập nhật real-time

**Thời gian ước tính (CPU, 500 ảnh):**
```
500 × 250ms trung bình = 125 giây ≈ 2 phút
```

### 6.2.3. Test Video Import

**Video 60 giây, 5 scenes:**
```
1. probe_video: ~500ms
2. detect_scenes: ~2-5s (ffmpeg scan)
3. Phân tích ~15 frames AI: ~15 × 250ms = 3.75s
4. Thumbnail generation: ~200ms
Tổng: ~7-10s
```

## 6.3. Test Search

### 6.3.1. Test Text Search

**Test case 1 — Query chính xác:**
```
Input:  "xe máy đường phố"
Expect: Trả về ảnh có object "motorcycle" hoặc 
        ảnh có embedding tương tự với concept "xe máy đường phố"
Similarity score: > 0.60
```

**Test case 2 — Query không khớp:**
```
Input:  "tàu vũ trụ trên mặt trăng"
Expect: Ít hoặc không có kết quả (score < 0.60)
```

**Test case 3 — Tiếng Việt có dấu:**
```
Input:  "ảnh gia đình" vs "anh gia dinh"
Expect: Kết quả khác nhau do normalize lowercase nhưng 
        giữ nguyên dấu (PhoBERT sensitive with diacritics)
```

### 6.3.2. Test Image-to-Image Search

```
Input:  Ảnh chụp con chó
Expect: Top-K ảnh trong DB có similarity score > 0.60
        Phần lớn kết quả phải chứa chó hoặc động vật
```

### 6.3.3. Test Combined Search

```
Input: text="bãi biển" + image=<sunset photo>
Expect: Intersection của text results và image results
        score = (text_score + image_score) / 2
```

## 6.4. Test Duplicate Detection

### 6.4.1. SHA-256 exact duplicate

**Test setup:**
```bash
cp photo.jpg photo_copy.jpg  # Exact duplicate
```
**Kỳ vọng:** Cả 2 xuất hiện trong DuplicatesView với `reason: "Trùng Hash"`

### 6.4.2. pHash near-duplicate

```bash
# Resize ảnh → pHash thay đổi ít, Hamming distance nhỏ
convert photo.jpg -resize 800x600 photo_resized.jpg
```
**Kỳ vọng:** Xuất hiện trong DuplicatesView với `reason: "Ảnh gần giống nhau (pHash)"`

### 6.4.3. Semantic duplicate

```bash
# Hai ảnh chụp cùng chủ thể, góc hơi khác
```
**Kỳ vọng:** Nếu cosine_similarity >= 0.92 → được nhóm với `reason: "Nội dung tương tự (AI)"`

## 6.5. Test People Recognition

### 6.5.1. Known identity (có trong face_db)

**Setup:** Thêm ảnh tham chiếu vào `assets/face_db/Alice/alice1.jpg`

**Test:**
1. Import ảnh có mặt Alice
2. Kỳ vọng: face được nhận diện với `name: "Alice"`, `face_id: <uuid_v5_for_Alice>`
3. PeopleView hiển thị "Alice" với photo count đúng

### 6.5.2. Unknown identity (session grouping)

**Test:**
1. Import 10 ảnh của cùng 1 người không có trong face_db
2. Kỳ vọng: Tất cả 10 ảnh được gom vào cùng face_id (UUID ngẫu nhiên)
3. User có thể đặt tên từ PeopleView → `cmd_name_person`

## 6.6. Đánh giá chất lượng tìm kiếm

### 6.6.1. Precision@K

Với tập test gồm 100 query text và ground truth (ảnh liên quan):

| K | Precision@K ước tính |
|---|---------------------|
| 5 | ~75-85% |
| 10 | ~65-75% |
| 20 | ~55-65% |

> **Chú ý:** Chưa có benchmark chính thức — đây là ước tính dựa trên model quality (CLIP-style, fine-tuned Vietnamese).

### 6.6.2. Recall và Threshold

```
threshold=0.6 → High precision, low recall (chỉ lấy ảnh rất liên quan)
threshold=0.4 → Lower precision, higher recall (lấy nhiều hơn nhưng noise)
```

---

# Chương 7: Các thách thức và định hướng mở rộng

## 7.1. Các thách thức hiện tại

### 7.1.1. Hiệu suất với dataset lớn

**Vấn đề:** Vector search trong SurrealDB thực hiện **full scan O(n)**:
```sql
-- Không có ANN index, mỗi search phải scan toàn bộ embedding table
SELECT ..., vector::similarity::cosine(vec, $qvec) FROM embedding WHERE score >= thresh
```

**Ảnh hưởng:**
- 10K ảnh: < 500ms (ổn)
- 100K ảnh: 2-5s (chậm)
- 1M ảnh: > 30s (không chấp nhận được)

**Giải pháp đề xuất:**
- Tích hợp HNSW index (SurrealDB v3 sắp hỗ trợ)
- Thay thế bằng vector database chuyên dụng: Qdrant, Milvus, Weaviate
- Hoặc FAISS (Facebook AI Similarity Search) trong process

### 7.1.2. Xử lý AI tuần tự (không song song)

**Vấn đề hiện tại:**
```rust
// image_ingest.rs - Thread 2 AI processing: sequential!
for (path, media_id, is_video) in to_process {
    process_image_file(&path, &media_id, ...).await;  // Tuần tự
}
```

**Lý do:** Model chia sẻ `&mut self`, không thể clone session ONNX. Đồng thời, inference trên 1 core hiệu quả hơn context switching.

**Giải pháp:**
- Sử dụng rayon thread pool với cloned models
- Batch inference (nhiều ảnh trong 1 forward pass)
- Tách phần preprocess (parallel) với inference (sequential)

### 7.1.3. Realtime Indexing

**Vấn đề:** FS Watcher debounce 2s → delay tối thiểu 2s trước khi xử lý file mới:
```rust
const DEBOUNCE_MS: u64 = 2000;
```

**Cải tiến:** Điều chỉnh debounce dynamically dựa trên batch size.

### 7.1.4. Tiêu thụ RAM khi Dataset lớn

```rust
// Toàn bộ session_faces giữ trong RAM
pub session_faces: Vec<(Vec<f32>, String)>  // Growing unbounded!
```

Với nhiều ảnh người (>10K unknowns), `session_faces` có thể chiếm nhiều RAM.

**Giải pháp:** Limit size + LRU eviction.

### 7.1.5. Face Recognition Accuracy

Threshold cứng `COSINE_THRESHOLD = 0.33` có thể không tối ưu cho:
- Ảnh chụp ở góc độ cực đoan (nghiêng >45°)
- Ảnh độ phân giải thấp (< 50px face)
- Ảnh có che khuất (mask, kính đậm)

**Giải pháp:** Adaptive threshold, ensemble models, multi-view matching.

### 7.1.6. Hỗ trợ HEIC/AVIF

```rust
pub const IMAGE_EXTENSIONS: &[&str] = &[..., "heic", "avif"];
```

Rust crate `image` hỗ trợ HEIC/AVIF còn hạn chế, cần thêm native decoder.

## 7.2. Định hướng mở rộng

### 7.2.1. Vector Database Chuyên dụng

Thay thế SurrealDB embedding table bằng:

```
┌─────────────────┐     ┌─────────────────┐
│   SurrealDB     │     │     Qdrant       │
│  (media, person,│ →   │  (embeddings)   │
│   config...)    │     │  HNSW index      │
└─────────────────┘     └─────────────────┘
```

**Qdrant** (Rust native): hỗ trợ HNSW, payload filtering, real-time upsert.

### 7.2.2. GPU Acceleration

```rust
// Thêm CUDA execution provider cho ONNX Runtime
use ort::ExecutionProvider;
let session = Session::builder()
    .with_execution_providers([CUDAExecutionProvider::default()])?
    .build()?;
```

Với NVIDIA GPU:
- YOLO inference: ~5-10x faster
- AuraModel: ~3-5x faster

### 7.2.3. Cloud Sync

Đồng bộ hoá bộ sưu tập ảnh giữa nhiều thiết bị:
```
Device A                    Cloud Storage
  ├── Upload embeddings  →  S3/R2/GCS
  ├── Sync media metadata → PostgreSQL
  └── P2P sync (local network) ← Device B
```

### 7.2.4. Better AI Models

| Lĩnh vực | Hiện tại | Cải tiến đề xuất |
|---------|---------|-----------------|
| Vision embedding | vi-sclir (CLIP-based) | SigLIP, EVA-CLIP |
| Object detection | YOLOv8n-seg | YOLOv10, RT-DETR |
| Face detection | YuNet 2022 | RetinaFace, SCRFD |
| Face recognition | SFace 2021 | ArcFace, AdaFace |
| Caption generation | — | BLIP-2, LLaVA |

### 7.2.5. Semantic Segmentation UI

Hiển thị mask trên ảnh khi hover (đã có mask_rle trong DB):
```javascript
// Frontend: decode RLE mask và overlay trên canvas
function decodeMaskRle(rle, width, height) {
    const mask = new Uint8Array(width * height);
    for (const [offset, length] of rle) {
        mask.fill(1, offset, offset + length);
    }
    return mask;
}
```

### 7.2.6. Multi-language Support

PhoBERT tokenizer hiện tại chỉ hỗ trợ **tiếng Việt**.
Để hỗ trợ đa ngôn ngữ: sử dụng multilingual CLIP (mCLIP) hoặc xlm-roberta.

### 7.2.7. Distributed Architecture

```
┌──────────────────────────────────┐
│         AuraSeek Client          │
│  (Lightweight React + Tauri)     │
└──────────────┬───────────────────┘
               │ HTTP/gRPC
┌──────────────▼───────────────────┐
│         AuraSeek Server          │
│  Rust backend (no GUI)           │
│  ├── AI Pipeline                 │
│  ├── Vector Search               │
│  └── REST API                    │
└──────────────┬───────────────────┘
               │
┌──────────────▼───────────────────┐
│     Distributed Storage          │
│  ├── Qdrant (vectors)            │
│  ├── PostgreSQL (metadata)       │
│  └── Object Store (media files)  │
└──────────────────────────────────┘
```

### 7.2.8. AI Caption & Auto-tagging

Tích hợp vision-language model để tự động sinh caption:
```
Ảnh → LLaVA / BLIP-2 → Caption: "Hai trẻ em đang chơi bóng..."
                     → Tags: ["trẻ em", "ngoài trời", "bóng đá"]
```

Caption và tags có thể được đưa vào embedding hoặc full-text index để cải thiện search recall.

### 7.2.9. Privacy & Security

- **Mã hoá local database:** Encrypt `auraseek_data/` với user key
- **Face blur/anonymization:** Option ẩn mặt trong export
- **Access control:** PIN/biometric để mở app (`cmd_authenticate_os` đã có cơ sở)

---

# Phụ lục A: Architecture Diagram (ASCII)

```
┌─────────────────────────────────────────────────────────────────────┐
│                         USER INTERACTIONS                           │
│                                                                     │
│  [Import Folder] [Drag & Drop] [Clipboard] [Search] [Browse]       │
└───────────────────────────────┬─────────────────────────────────────┘
                                ↓
┌───────────────────────────────▼─────────────────────────────────────┐
│                    REACT UI LAYER (TypeScript)                      │
│                                                                     │
│  App.tsx                                                            │
│  ├── AppSidebar ──── Navigation (Timeline, People, Albums...)       │
│  ├── AppTopbar  ──── SearchBar + Filters + SyncStatus              │
│  └── Views:                                                         │
│      ├── TimelineView: Grid photos by year/month                    │
│      ├── SearchResultsView: Cards with similarity score             │
│      ├── PeopleView: Face cluster grid                              │
│      ├── DuplicatesView: Duplicate groups                           │
│      └── (Albums, Trash, Hidden, Gallery...)                        │
│                                                                     │
│  lib/api.ts → AuraSeekApi.invoke("cmd_*")                          │
└───────────────────────────────┬─────────────────────────────────────┘
                                ↓  Tauri IPC Bridge
┌───────────────────────────────▼─────────────────────────────────────┐
│                   RUST BACKEND LAYER (Tauri)                        │
│                                                                     │
│  main.rs: Tauri Commands                                            │
│  ┌─────────────┐  ┌──────────────┐  ┌────────────┐  ┌───────────┐  │
│  │  ingest/    │  │  processor/  │  │  search/   │  │   db/     │  │
│  │             │  │              │  │            │  │           │  │
│  │image_ingest │  │ pipeline.rs  │  │ pipeline   │  │ surreal   │  │
│  │video_ingest │  │   (Engine)   │  │ text_search│  │ models    │  │
│  └──────┬──────┘  └──────┬───────┘  │ img_search │  │operations │  │
│         │                │          └─────┬──────┘  └─────┬─────┘  │
│         └────────────────┴────────────────┘               │         │
│                          ↓                                │         │
│  ┌─────────────────────────────────┐              ┌───────▼──────┐  │
│  │         model/ Layer            │              │  SurrealDb   │  │
│  │                                 │              │  (Client)    │  │
│  │  ┌─────────┐ ┌──────┐ ┌──────┐  │              └──────────────┘  │
│  │  │  Aura   │ │ YOLO │ │ Face │  │                                 │
│  │  │  Model  │ │Model │ │Model │  │                                 │
│  │  │ONNX RT  │ │ONNXRT│ │OpenCV│  │                                 │
│  │  └─────────┘ └──────┘ └──────┘  │                                 │
│  └─────────────────────────────────┘                                 │
│                                                                     │
│  fs_watcher.rs: notify crate → OS file events                       │
│  surreal_sidecar.rs: manage SurrealDB child process                 │
│  Axum HTTP server: video streaming (port 8000-9000)                 │
└───────────────────────────────┬─────────────────────────────────────┘
                                ↓
┌───────────────────────────────▼─────────────────────────────────────┐
│                    INFRASTRUCTURE LAYER                             │
│                                                                     │
│  ┌─────────────────────┐    ┌──────────────────────────────────┐    │
│  │   SurrealDB Sidecar  │    │         AI Models (ONNX)         │    │
│  │   (surrealkv://)     │    │                                  │    │
│  │                     │    │  vision_vi-sclir.onnx            │    │
│  │  auraseek_data/     │    │  text_vi-sclir.onnx              │    │
│  │  ├── sstables/      │    │  yolo26n-seg.onnx                │    │
│  │  ├── vlog/          │    │  face_detection_yunet.onnx        │    │
│  │  └── wal/           │    │  face_recognition_sface.onnx     │    │
│  └─────────────────────┘    └──────────────────────────────────┘    │
│                                                                     │
│  ┌──────────────────────────────────────────────────────────────┐   │
│  │              Local File System (User Photos)                 │   │
│  │                                                              │   │
│  │  /home/user/Photos/                                          │   │
│  │  ├── IMG_001.jpg  IMG_002.jpg  ...  (Images)                │   │
│  │  ├── VID_001.mp4  VID_002.mov  ...  (Videos)                │   │
│  │  └── *.thumb.jpg                    (Video thumbnails cache) │   │
│  └──────────────────────────────────────────────────────────────┘   │
└─────────────────────────────────────────────────────────────────────┘
```

---

# Phụ lục B: Code References — Các file quan trọng

| File | Kích thước | Vai trò chính |
|------|-----------|--------------|
| `src-tauri/src/main.rs` | 42KB | Entry point, tất cả Tauri commands |
| `src-tauri/src/db/operations.rs` | 43KB | Database CRUD + vector search + duplicate detection |
| `src-tauri/src/ingest/image_ingest.rs` | 21KB | Image import pipeline |
| `src-tauri/src/ingest/video_ingest.rs` | 17KB | Video import pipeline |
| `src-tauri/src/model/face.rs` | 12KB | Face detection + recognition |
| `src-tauri/src/db/surreal.rs` | 10KB | DB connection + schema |
| `src-tauri/src/surreal_sidecar.rs` | 9KB | SurrealDB process management |
| `src-tauri/src/search/pipeline.rs` | 7KB | Search orchestration |
| `src-tauri/src/processor/pipeline.rs` | 7KB | AI engine |
| `src-tauri/src/processor/vision/yolo_postprocess.rs` | 7KB | YOLO NMS + mask |
| `src-tauri/src/processor/text/tokenizer.rs` | 6KB | PhoBERT BPE tokenizer |
| `src-tauri/src/db/models.rs` | 9KB | Data model structs |
| `src-tauri/src/fs_watcher.rs` | 7KB | FS monitoring |
| `src/App.tsx` | 22KB | React root component |
| `src/lib/api.ts` | 11KB | Tauri API bridge |

---

# Phụ lục C: Data Flow Diagram

```
                    ┌──────────────────────────────────────────┐
                    │             INPUT DATA                   │
                    │                                          │
                    │  .jpg .png .webp .heic .avif             │
                    │  .mp4 .mov .mkv .avi .webm               │
                    └──────────────┬───────────────────────────┘
                                   │
                    ┌──────────────▼───────────────────────────┐
                    │         PREPROCESSING                    │
                    │                                          │
                    │  Image: resize 256x256, normalize CHW    │
                    │  Image: letterbox 640x640 for YOLO       │
                    │  Video: ffprobe → detect scenes          │
                    │         extract representative frames    │
                    └──────────────┬───────────────────────────┘
                                   │
          ┌────────────────────────┼────────────────────────┐
          │                        │                        │
┌─────────▼──────────┐  ┌──────────▼──────────┐  ┌─────────▼──────────┐
│   YOLO Inference   │  │   Face Detection    │  │  Aura Inference    │
│                    │  │   & Recognition     │  │                    │
│  yolo26n-seg.onnx  │  │  YuNet + SFace      │  │ vision_vi-sclir    │
│  Input: [1,3,640,  │  │  Detector: 320x320  │  │ Input: [1,3,256,  │
│         640]       │  │  Recognizer: 112x   │  │        256]       │
│  Output: bboxes,   │  │         112          │  │ Output: [1, dim]  │
│          masks,    │  │  Embedding: 128-d    │  │                    │
│          classes   │  │  Threshold: 0.33    │  │                    │
└─────────┬──────────┘  └──────────┬──────────┘  └─────────┬──────────┘
          │                        │                        │
          │ Vec<DetectionRecord>   │ Vec<FaceGroup>         │ Vec<f32>
          │ {class, conf, bbox,    │  {face_id, name,       │ embedding
          │  mask_rle, mask_area}  │   conf, bbox, emb}     │ vector
          └────────────────────────┼────────────────────────┘
                                   │
                    ┌──────────────▼───────────────────────────┐
                    │           POSTPROCESSING                  │
                    │                                          │
                    │  NMS (IoU > 0.45)                        │
                    │  Mask reconstruction + RLE encoding       │
                    │  Face bbox coord transformation           │
                    │  Session face clustering (unknown faces)  │
                    └──────────────┬───────────────────────────┘
                                   │
                    ┌──────────────▼───────────────────────────┐
                    │         DATABASE STORAGE                 │
                    │                                          │
                    │  media table:                           │
                    │    UPDATE media SET                     │
                    │      objects = [...DetectionRecords],   │
                    │      faces   = [...FaceGroups],         │
                    │      processed = true                   │
                    │                                          │
                    │  embedding table:                       │
                    │    INSERT {media_id, source, vec}       │
                    │                                          │
                    │  person table:                          │
                    │    UPSERT {face_id, name, conf, bbox}   │
                    └──────────────┬───────────────────────────┘
                                   │
                    ┌──────────────▼───────────────────────────┐
                    │           SEARCH INDEX                   │
                    │                                          │
                    │  Text query → PhoBERT tokenize → embed  │
                    │  Image query → preprocess → embed        │
                    │                                          │
                    │  vector::similarity::cosine search       │
                    │  threshold: 0.60                         │
                    │  limit: 10,000                           │
                    │                                          │
                    │  → Vec<(media_id, similarity_score)>    │
                    └──────────────┬───────────────────────────┘
                                   │
                    ┌──────────────▼───────────────────────────┐
                    │           PRESENTATION                   │
                    │                                          │
                    │  Timeline: grouped by year/month         │
                    │  Search results: sorted by score         │
                    │  People: grouped by face_id              │
                    │  Duplicates: grouped by SHA/pHash/vector │
                    └──────────────────────────────────────────┘
```

---

# Phụ lục D: Glossary — Thuật ngữ

| Thuật ngữ | Định nghĩa |
|-----------|-----------|
| **Cosine Similarity** | Đo sự tương đồng giữa 2 vector bằng góc giữa chúng. Giá trị từ -1 đến 1, càng gần 1 càng giống nhau |
| **Embedding** | Vector số thực đại diện cho nội dung ngữ nghĩa của ảnh/văn bản trong không gian vector |
| **ONNX** | Open Neural Network Exchange — format chuẩn để lưu và chạy AI models |
| **BPE** | Byte-Pair Encoding — thuật toán tách từ thành subword units cho NLP |
| **NMS** | Non-Maximum Suppression — loại bỏ bounding box thừa trong object detection |
| **IoU** | Intersection over Union — đo độ chồng lấp giữa 2 bounding box |
| **RLE** | Run-Length Encoding — nén mask bitmap bằng cách lưu (offset, length) của runs |
| **pHash** | Perceptual Hash — hash của ảnh dựa trên DCT, ảnh tương tự có Hamming distance nhỏ |
| **SHA-256** | Secure Hash Algorithm 256-bit — fingerprint của file, 2 file giống hệt → cùng hash |
| **Letterbox** | Kỹ thuật resize ảnh giữ tỷ lệ, padding bằng màu xám ở 2 cạnh để đạt kích thước target |
| **CLIP** | Contrastive Language-Image Pre-training — kiến trúc học biểu diễn ảnh và văn bản trong cùng không gian |
| **PhoBERT** | Bidirectional Encoder Representations from Transformers — pretrained trên tiếng Việt |
| **SurrealKV** | Storage engine nhúng của SurrealDB v3, kiến trúc LSM-tree |
| **SSTable** | Sorted String Table — file lưu trữ data đã được sort, immutable |
| **WAL** | Write-Ahead Log — ghi operations vào log trước khi commit, đảm bảo durability |
| **Debounce** | Kỹ thuật delay xử lý event để gom nhiều event liên tiếp thành 1 batch |
| **Sidecar** | Tiến trình phụ chạy song song với ứng dụng chính (SurrealDB as sidecar) |
| **ANN** | Approximate Nearest Neighbor — tìm kiếm vector gần nhất một cách xấp xỉ (nhanh hơn exact) |
| **HNSW** | Hierarchical Navigable Small World — thuật toán ANN phổ biến, dùng trong Qdrant, FAISS |
| **Tauri** | Framework để xây dựng desktop app với web frontend và Rust backend |
| **IPC** | Inter-Process Communication — giao tiếp giữa frontend và backend trong Tauri |

---

*Tài liệu được tổng hợp từ phân tích trực tiếp source code AuraSeek v1.0.0 — Tháng 3 năm 2026.*

---

# Phụ lục E: Chi tiết các Tauri Commands

## E.1. cmd_init — Khởi tạo hệ thống

```rust
// src-tauri/src/main.rs
#[tauri::command]
async fn cmd_init(state: State<'_, AppState>, app: AppHandle) -> Result<String, String> {
    // Bước 1: SurrealDB sidecar
    let resource_dir = app.path().resource_dir()...;
    let data_dir = app.path().app_data_dir()?.join("auraseek_data");
    
    let (surreal_addr, surreal_child) = ensure_surreal(
        &resource_dir, &data_dir, "root", "root"
    )?;
    // Giữ child để cleanup khi app thoát
    *state.surreal_child.lock().await = surreal_child;
    
    // Bước 2: Kết nối DB
    let sdb = SurrealDb::connect(&surreal_addr, "root", "root").await?;
    *state.db.lock().await = Some(sdb.clone());
    
    // Bước 3: Load AI models
    let config = EngineConfig::new_with_dir(&asset_dir);
    let engine = AuraSeekEngine::new(config)?;
    *state.engine.lock().await = Some(engine);
    
    // Bước 4: Axum streaming server
    let stream_port = start_video_stream_server();
    *state.stream_port.lock().await = Some(stream_port);
    
    // Bước 5: FS Watcher
    let source_dir = DbOperations::get_source_dir(&sdb).await?;
    if !source_dir.is_empty() {
        let handle = start_watching(source_dir, db.clone(), engine.clone(), ...);
        *state.watcher.lock().await = Some(handle);
    }
    
    Ok("Engine + DB ready".to_string())
}
```

**Lifecycle Engine init:**
```
cmd_init() được gọi 1 lần khi app khởi động
         ↓
AuraSeekEngine::new(config)
  ├── AuraModel::new(vision_path, text_path)  [5-30s, tùy model size]
  ├── TextProcessor::new(vocab_path, bpe_path)  [1-3s]
  ├── YoloModel::new(yolo_path)  [2-5s]
  ├── FaceModel::new(yunet_path, sface_path)  [3-10s]
  └── FaceDb::build(face_db_path, &mut face_model)  [varies]
```

## E.2. cmd_download_models — Tải model

```rust
#[tauri::command]
async fn cmd_download_models(state: State<'_, AppState>, app: AppHandle) -> Result<(), String> {
    // Emit progress events đến frontend
    let emit_progress = |file: &str, progress: f64, msg: &str| {
        app.emit("model-download-progress", ModelDownloadEvent {
            file: file.to_string(),
            progress,
            message: msg.to_string(),
            done: false, error: "".to_string(),
            ...
        })
    };
    
    // Download từng model file
    // src-tauri/src/downloader.rs → reqwest streaming download
    for (url, dest_path) in MODEL_URLS {
        download_with_progress(url, dest_path, emit_progress).await?;
    }
    
    app.emit("model-download-progress", ModelDownloadEvent { done: true, ... });
}
```

## E.3. cmd_scan_folder — Import folder

```rust
#[tauri::command]
async fn cmd_scan_folder(
    state: State<'_, AppState>,
    app: AppHandle,
    source_path: String,
) -> Result<IngestSummary, String> {
    let db = state.db.clone();
    let engine = state.engine.clone();
    
    image_ingest::ingest_folder(
        source_path,
        db,
        engine,
        Some(app),
        state.thumb_cache_dir.clone(),
    ).await
    .map_err(|e| e.to_string())
}
```

**IngestSummary response:**
```json
{
  "total_found": 250,
  "newly_added": 187,
  "skipped_dup": 63,
  "errors": 0
}
```

## E.4. cmd_get_timeline — Lấy timeline

```rust
#[tauri::command]
async fn cmd_get_timeline(
    state: State<'_, AppState>,
    limit: Option<usize>,
) -> Result<Vec<TimelineGroup>, String> {
    let db_guard = state.db.lock().await;
    let db = db_guard.as_ref().ok_or("DB not connected")?;
    let source_dir = state.source_dir.lock().await.clone();
    
    DbOperations::get_timeline(db, limit.unwrap_or(50000), &source_dir)
        .await
        .map_err(|e| e.to_string())
}
```

**TimelineGroup response structure:**
```json
[
  {
    "label": "Tháng 3 năm 2026",
    "year": 2026,
    "month": 3,
    "day": null,
    "items": [
      {
        "media_id": "media:abc123",
        "file_path": "/home/user/Photos/IMG_001.jpg",
        "media_type": "image",
        "width": 4032,
        "height": 3024,
        "created_at": "2026-03-10T14:23:45+00:00",
        "objects": ["person", "car"],
        "faces": ["Alice"],
        "face_ids": ["uuid-alice"],
        "favorite": false,
        "detected_objects": [
          {
            "class_name": "person",
            "conf": 0.92,
            "bbox": {"x": 100, "y": 50, "w": 200, "h": 400},
            "mask_rle": [[1024, 50], [1124, 45], ...]
          }
        ],
        "detected_faces": [
          {
            "face_id": "uuid-alice",
            "name": "Alice",
            "conf": 0.95,
            "bbox": {"x": 120, "y": 60, "w": 80, "h": 100}
          }
        ],
        "thumbnail_path": null
      }
    ]
  }
]
```

## E.5. cmd_search_text — Text Search

```rust
#[tauri::command]
async fn cmd_search_text(
    state: State<'_, AppState>,
    query: String,
    filters: Option<SearchFilters>,
) -> Result<Vec<SearchResult>, String> {
    let db = db_guard.as_ref()?;
    let engine = engine_guard.as_mut()?;
    
    let search_query = SearchQuery {
        mode: SearchMode::Text,
        text: Some(query),
        image_path: None,
        filters: filters.unwrap_or_default().into(),
    };
    
    SearchPipeline::run(&search_query, engine, db, &source_dir).await
}
```

**SearchResult response:**
```json
[
  {
    "media_id": "media:xyz789",
    "similarity_score": 0.847,
    "file_path": "/home/user/Photos/beach.jpg",
    "media_type": "image",
    "width": 3840,
    "height": 2160,
    "metadata": {
      "created_at": "2025-08-15T10:30:00+00:00",
      "objects": ["person", "umbrella"],
      "faces": []
    },
    "detected_objects": [...],
    "detected_faces": [],
    "thumbnail_path": null
  }
]
```

---

# Phụ lục F: Chi tiết xử lý Video

## F.1. Scene Detection Algorithm

```rust
// src-tauri/src/ingest/video_ingest.rs
const SCENE_THRESHOLD: f64 = 0.11;  // 0=không thay đổi, 1=thay đổi hoàn toàn

pub fn detect_scenes(video_path: &str, fps: f64) -> Result<Vec<u64>> {
    // Dùng ffmpeg filter để detect scene changes
    let filter = format!("select='gt(scene,{})',showinfo", SCENE_THRESHOLD);
    
    let output = Command::new("ffmpeg")
        .args(["-i", video_path, "-vf", &filter, "-vsync", "vfr", "-f", "null", "-"])
        .stderr(Stdio::piped())
        .output()?;
    
    // Parse pts_time từ stderr của showinfo filter
    // Format: "[Parsed_showinfo_1 @ 0x...] n:0 ... pts_time:1.234 ..."
    let stderr = String::from_utf8_lossy(&output.stderr);
    let mut cuts: Vec<u64> = Vec::new();
    for line in stderr.lines() {
        if !line.contains("pts_time") { continue; }
        if let Some(t) = parse_pts_time(line) {
            cuts.push((t * fps).round() as u64);
        }
    }
    cuts.sort_unstable();
    cuts.dedup();
    Ok(cuts)
}
```

**Ví dụ phân tích video 5 phút:**
```
Video: 300s, 30fps = 9000 frames
Scene cuts detected: [90, 450, 1230, 2700, 5400, 7200]

Scenes:
  Scene 0: frames [0, 89]     → len=90, sample at 18, 45, 72
  Scene 1: frames [90, 449]   → len=360, sample at 162, 270, 378
  Scene 2: frames [450, 1229] → len=780, sample at 606, 840, 1074
  ...

Frame jobs: [18, 45, 72, 162, 270, 378, 606, 840, 1074, ...]
```

## F.2. Brightness Filter

```rust
// Kiểm tra độ sáng frame để tránh frame đen/trắng
pub fn is_good_brightness(path: &Path) -> (bool, f64) {
    let img = image::open(path)?.to_rgba8();
    let (w, h) = img.dimensions();
    
    let mut total_luma = 0u64;
    for pixel in img.pixels() {
        // Rec. 601 luma formula: L = 0.299R + 0.587G + 0.114B
        let luma = (pixel[0] as u64 * 299 + pixel[1] as u64 * 587 + pixel[2] as u64 * 114) / 1000;
        total_luma += luma;
    }
    
    let avg_luma = total_luma as f64 / (w * h) as f64;
    // Quá tối (< 25) hoặc quá sáng (> 240) → bad frame
    let is_good = avg_luma >= 25.0 && avg_luma <= 240.0;
    (is_good, avg_luma)
}
```

## F.3. Embedding Aggregation cho Video

```rust
// Với video: nhiều frame → nhiều embeddings
// Mỗi embedding được lưu riêng với frame_ts và frame_idx
// Cho phép search tìm đến đúng timestamp trong video

for frame_idx in &frame_jobs {
    let timestamp = *frame_idx as f64 / fps;
    DbOperations::insert_embedding(
        sdb, media_id, "video_frame",
        Some(timestamp),            // ← timestamp trong video
        Some(*frame_idx as u32),    // ← frame index
        output.vision_embedding.clone(),
    ).await?;
}

// Khi search tìm video, score cao nhất trong tất cả frames
// được dùng để rank video đó
```

---

# Phụ lục G: Cấu trúc SurrealDB Schema đầy đủ

## G.1. Migration Logic

```rust
// src-tauri/src/db/surreal.rs - ensure_schema()

// Migration: xóa các field cũ không còn dùng
REMOVE FIELD IF EXISTS file.path ON media;  // Path đã chuyển sang config
REMOVE FIELD IF EXISTS source ON media;     // Source_dir lưu trong config_auraseek

// Đảm bảo tất cả existing records có field mới
UPDATE media SET deleted_at = NONE, is_hidden = false WHERE is_hidden = NONE;
```

## G.2. Query Examples

**Lấy tất cả ảnh có object "dog":**
```sql
SELECT * FROM media
WHERE objects.*.class_name CONTAINS 'dog'
  AND deleted_at = NONE
  AND is_hidden = false
ORDER BY metadata.created_at DESC
LIMIT 100
```

**Đếm số ảnh mỗi người:**
```sql
SELECT
    face_id, name, thumbnail,
    (SELECT count() FROM media
     WHERE faces.*.face_id CONTAINS $parent.face_id
       AND deleted_at = NONE
       AND is_hidden = false
     GROUP ALL)[0].count AS photo_count
FROM person
ORDER BY photo_count DESC
```

**Vector search:**
```sql
SELECT
    media_id,
    vector::similarity::cosine(vec, $qvec) AS score
FROM embedding
WHERE vector::similarity::cosine(vec, $qvec) >= 0.6
ORDER BY score DESC
LIMIT 10000
```

**Đặt tên cho người (cascade update):**
```sql
-- Update bảng person
UPDATE person SET name = 'Alice' WHERE face_id = 'uuid-123';

-- Update tất cả media records có face này
UPDATE media SET
    faces = faces.map(|$f| IF $f.face_id = 'uuid-123'
                          THEN $f.{*, name: 'Alice'}
                          ELSE $f END)
WHERE faces.*.face_id CONTAINS 'uuid-123';
```

---

# Phụ lục H: Cấu hình Tauri

## H.1. tauri.conf.json

```json
{
  "tauri": {
    "bundle": {
      "identifier": "com.auraseek.app",
      "version": "1.0.0",
      "resources": [
        "assets/models/*.onnx",
        "assets/tokenizer/*",
        "binaries/surreal*"
      ]
    },
    "security": {
      "csp": "...",
      "assetProtocol": {
        "enable": true,
        "scope": ["**"]
      }
    }
  }
}
```

## H.2. Capabilities

```json
// src-tauri/capabilities/default.json
{
  "identifier": "default",
  "description": "Default capabilities",
  "windows": ["main"],
  "permissions": [
    "core:default",
    "core:path:default",
    "core:window:default",
    "opener:default",
    "protocol:asset"
  ]
}
```

## H.3. Build Script (build.rs)

```rust
// src-tauri/build.rs
// Tự động download model files khi build
// Dùng ureq (sync HTTP) để tải trong build time
fn main() {
    tauri_build::build();
    // download_models_if_needed();
}
```

---

# Phụ lục I: Cấu trúc React Components

## I.1. Component Hierarchy

```
App.tsx
├── SelectionProvider (Context)
│   └── TooltipProvider
│       └── SidebarProvider
│           ├── AppSidebar
│           │   ├── SidebarHeader (logo, app name)
│           │   ├── SidebarContent
│           │   │   ├── Nav: Timeline, Videos
│           │   │   ├── Nav: People, Albums, Duplicates
│           │   │   └── Nav: Favorites, Trash, Hidden
│           │   └── SidebarFooter (Settings, source dir)
│           │
│           └── main (flex column)
│               ├── AppTopbar
│               │   ├── SearchInput (text query)
│               │   ├── SearchImageDropzone (image query)
│               │   ├── FilterPanel (object, face, date, type)
│               │   ├── SyncStatusIndicator
│               │   └── SelectionModeToggle
│               │
│               └── [renderView()]
│                   ├── TimelineView
│                   │   ├── TimelineGroupHeader (label, count)
│                   │   └── PhotoGrid
│                   │       └── PhotoCard (thumbnail, overlays)
│                   │
│                   ├── SearchResultsView
│                   │   └── SearchResultCard (similarity badge)
│                   │
│                   ├── PeopleView
│                   │   └── PersonCard (face crop, name, count)
│                   │
│                   └── DuplicatesView
│                       └── DuplicateGroupCard
│                           └── DuplicateItemCard (size, thumb)
```

## I.2. Photo.type.ts — Type Definition

```typescript
// src/types/photo.type.ts
export interface Photo {
    id: string;              // media_id
    url: string;             // asset:// URL cho display
    takenAt: string;         // ISO datetime
    createdAt: string;
    sizeBytes: number;
    width: number;
    height: number;
    objects: string[];       // class names từ YOLO
    faces: string[];         // tên người đã nhận diện
    faceIds: string[];       // UUIDs của face
    type: "photo" | "video";
    labels: string[];        // = objects
    favorite: boolean;
    detectedObjects: DetectedObject[];  // Full detection data (bbox + mask)
    detectedFaces: DetectedFace[];
    thumbnailUrl?: string;   // Video thumbnail URL
    filePath: string;        // Absolute filesystem path
}
```

## I.3. useSelection Hook

```typescript
// src/contexts/SelectionContext.tsx
const useSelection = () => {
    const context = useContext(SelectionContext);
    return {
        selectedIds: Set<string>,
        isSelected: (id: string) => boolean,
        toggleSelect: (id: string) => void,
        selectAll: (ids: string[]) => void,
        deselectAll: () => void,
        selectedCount: number,
    };
};
```

---

# Phụ lục J: Logging System

## J.1. Log Macros

```rust
// src-tauri/src/utils/logger.rs
// Custom macros với timestamp và level

macro_rules! log_info {
    ($($arg:tt)*) => {
        println!("[INFO] {}", format!($($arg)*));
    }
}

macro_rules! log_warn {
    ($($arg:tt)*) => {
        eprintln!("[WARN] {}", format!($($arg)*));
    }
}

macro_rules! log_error {
    ($($arg:tt)*) => {
        eprintln!("[ERROR] {}", format!($($arg)*));
    }
}
```

## J.2. Log Samples

**Khởi động thành công:**
```
[INFO] ��️  Starting SurrealDB | binary=./surreal port=8000 uri=surrealkv://...
[INFO] ✅ SurrealDB spawned (pid=12345)
[INFO] ✅ SurrealDB ready on port 8000
[INFO] 🔌 Connecting to SurrealDB via WS: ws://127.0.0.1:8000...
[INFO] ✅ WS connection established
[INFO] 🔑 Authenticating as user 'root'...
[INFO] �� Schema ready: media, embedding, person, config_auraseek, search_history
[INFO] loading ai models
[INFO] model: assets/models/vision_vi-sclir.onnx     | provider: CPU
[INFO] model: assets/models/text_vi-sclir.onnx       | provider: CPU
[INFO] yolo: 80 classes loaded
[INFO] model: assets/models/face_detection_yunet.onnx | provider: CPU
[INFO] model: assets/models/face_recognition_sface.onnx | provider: CPU
[INFO] face_db: total identities loaded: 3
[INFO] 👁️  FS watcher started on: /home/user/Photos
```

**Ingest ảnh:**
```
[INFO] 📂 Ingest started: /home/user/Photos | 127 images + 8 videos found
[INFO] 🤖 [AI 1/135] Processing: IMG_20260310_142345.jpg
[INFO]   ✅ Done in 312ms | objects=2 faces=1 embed_dims=512
[INFO]   👤 Upserting person face_id=uuid-alice conf=0.956
[INFO] 🤖 [AI 2/135] Processing: video_001.mp4
[INFO] 🎥 Video probe: video_001.mp4 fps=29.97 frames=5394
[INFO] 🎬 4 scenes detected
[INFO] 🖼️  12 frames to process for video_001.mp4
[INFO]   🖼  Frame 108 @ 3.60s | obj=3 face=0 emb=512
[INFO]   ✅ Frame 108 @ 3.60s embedded
[INFO] 🎥 Video done: 12 embeds, 3 objects, 0 faces | video_001.mp4
[INFO] ✅ Ingest complete: 135 new, 0 skipped, 0 errors, 135 AI processed
```

**Vector search:**
```
[INFO] 🔤 Text query: 'xe máy đường phố' → normalized: 'xe máy đường phố'
[INFO] 🔤 Token ids (5 real / 64 max): [0, 4521, 892, 2341, 567, 2]
[INFO] 🔍 [SearchPipeline::run] mode=Text text='xe máy đường phố'
[INFO] 🔍 Found: 23 results (score >= 0.60)
```

---

# Phụ lục K: Quy trình Deploy và Đóng gói

## K.1. Development Mode

```bash
# Cài đặt dependencies
yarn install

# Chạy development server
yarn tauri dev
# Hoặc:
npm run tauri dev
```

Tauri sẽ:
1. Khởi động Vite dev server (Frontend hot-reload)
2. Build Rust backend (debug mode, incremental)
3. Mở app window với WebView

**Yêu cầu:**
- Rust toolchain (stable)
- Node.js >= 18
- OpenCV development libraries
- ffmpeg binary trong PATH
- SurrealDB binary trong `src-tauri/binaries/`

## K.2. Production Build

```bash
# Build production bundle
yarn tauri build

# Output:
# - Linux: src-tauri/target/release/bundle/appimage/auraseek_1.0.0_amd64.AppImage
# - Windows: src-tauri/target/release/bundle/msi/auraseek_1.0.0_x64.msi
# - macOS: src-tauri/target/release/bundle/dmg/AuraSeek_1.0.0.dmg
```

## K.3. Bundled Resources

Các file được bundle vào binary/installer:
```
tauri.conf.json → "resources":
  ├── assets/models/yolo26n-seg.onnx         (10.7 MB)
  ├── assets/models/face_detection_yunet.onnx (345 KB)
  ├── assets/models/face_recognition_sface.onnx (36.9 MB)
  ├── assets/tokenizer/vocab.txt              (895 KB)
  ├── assets/tokenizer/bpe.codes              (1.1 MB)
  └── binaries/surreal[.exe]                  (~80 MB)
```

Vision/text ONNX models không bundle → download on first run.

---

# Phụ lục L: Các công nghệ core và lý do chọn

## L.1. Tauri v2 — Lý do chọn

| So sánh | Tauri | Electron |
|--------|-------|----------|
| Bundle size | ~5-15 MB | ~80-200 MB |
| RAM footprint | Thấp hơn | Cao hơn |
| Backend language | Rust (safe, fast) | Node.js |
| Security | Granular permissions | Rộng hơn |
| Native OS APIs | Tốt | OK |

Tauri phù hợp vì:
1. Backend Rust cho phép dùng OpenCV, ONNX Runtime native
2. Bundle size nhỏ (không bundle Chromium)
3. Security model tốt cho app xử lý file local

## L.2. SurrealDB v3 — Lý do chọn

| Feature | SurrealDB | SQLite | PostgreSQL |
|---------|----------|--------|-----------|
| Embedded sidecar | ✅ | ✅ | ❌ |
| Schemafull tables | ✅ | Limited | ✅ |
| JSON/array fields | Native | Via JSON | Via JSONB |
| Vector similarity | `vector::similarity::cosine` built-in | ❌ | Via pgvector |
| Graph queries | ✅ | ❌ | Limited |
| Record links | ✅ | ❌ | Via FK |

SurrealDB được chọn vì tích hợp vector search built-in, không cần external service.

## L.3. ONNX Runtime — Lý do chọn

- **Cross-platform:** Chạy được trên Linux/Windows/macOS
- **CPU + GPU:** Tự động chọn execution provider phù hợp
- **Standard format:** Nhiều model PyTorch/TensorFlow có thể export sang ONNX
- **Rust bindings:** `ort` crate native cho Rust

## L.4. OpenCV — Lý do dùng cho Face

- OpenCV có sẵn DNN module với YuNet và SFace
- `align_crop()` — face alignment chuyên nghiệp (5-point landmark)
- CUDA acceleration built-in
- `opencv` Rust crate (0.92) ổn định

## L.5. ffmpeg — Video Processing

```rust
// Sử dụng ffmpeg subprocess (không dùng FFI)
Command::new("ffmpeg")
    .args(["-i", video_path, "-vf", "select='gt(scene,0.11)'", ...])
    .output()
```

Lý do: ffmpeg robust nhất cho video processing, hỗ trợ hầu hết codec, scene detection filter sẵn có.

---

# Phụ lục M: Error Handling Strategy

## M.1. Rust Error Model

```rust
// Toàn bộ backend dùng anyhow::Result<T>
use anyhow::{Result, Context};

pub async fn some_operation() -> Result<String> {
    let data = read_file(path).context("Failed to read model file")?;
    let parsed = parse_data(&data).context("Invalid model format")?;
    Ok(parsed)
}
```

## M.2. Tauri Command Error Propagation

```rust
// Commands return Result<T, String> để serialize qua IPC
#[tauri::command]
async fn cmd_search_text(...) -> Result<Vec<SearchResult>, String> {
    operation().await.map_err(|e| e.to_string())
}
```

## M.3. Frontend Error Handling

```typescript
// src/App.tsx
try {
    const results = await AuraSeekApi.searchText(query);
    setSearchResults(results);
} catch (err) {
    console.error("[AuraSeek] ❌ Search failed:", err);
    setSearchResults([]);  // Show empty state, don't crash
    setRoute({ view: "search_results" });
}
```

## M.4. Graceful Degradation

```rust
// Face model optional — app vẫn chạy nếu model load failed
let mut face = match FaceModel::new(&config.yunet_path, &config.sface_path) {
    Ok(m)  => Some(m),
    Err(e) => {
        log_warn!("face model failed to load: {}", e);
        None  // App tiếp tục chạy, chỉ không có face recognition
    }
};

// RAM check trước khi process
let ram_pct = available_ram_percent();
if ram_pct < 40.0 {
    skip_batch("Not enough RAM");
    continue;
}
```

---

# Phụ lục N: Performance Tuning Tips

## N.1. Tối ưu cho bộ sưu tập lớn

**1. Tăng SurrealDB memory:**
```
Mặc định SurrealKV quản lý cache tự động.
Cân nhắc tăng OS-level cache (sufficient RAM).
```

**2. Giảm embedding dimension (nếu fine-tune model):**
```
Hiện tại: dim phụ thuộc vào model (512 hoặc 1024)
Giảm dim → faster search, less memory, lower accuracy
```

**3. Tăng debounce để batch tốt hơn:**
```rust
// fs_watcher.rs
const DEBOUNCE_MS: u64 = 5000;  // Tăng từ 2000ms lên 5000ms
// Gom nhiều file hơn mỗi batch → ít overhead hơn
```

## N.2. Tối ưu cho YOLO

```rust
// Hiện tại: conf_thresh=0.25, iou_thresh=0.45
// Tăng conf_thresh để giảm số detection → nhanh hơn, ít noise
YoloProcessor::postprocess(&raw, &lb, 0.35, 0.45)
```

## N.3. Memory Management

```rust
// AuraSeekEngine: không clone session, dùng shared Arc<Mutex<>>
// Engine được giữ trong Arc để share giữa các async tasks
let engine = Arc::new(Mutex::new(Some(AuraSeekEngine::new(config)?)));
```

---

*Tài liệu kỹ thuật AuraSeek v1.0.0 — Hoàn thành*  
*Được phân tích và soạn thảo dựa trên source code thực tế của project.*  
*Ngày: Tháng 3 năm 2026*
