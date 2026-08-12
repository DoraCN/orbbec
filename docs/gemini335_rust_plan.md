# 奥比中光 Gemini 335 深度相机接入方案

- 日期：2026-08-12
- 系统：Ubuntu 22.04 (内核 6.8.0-136-generic HWE)
- 相机：Orbbec Gemini 335 (G40155-170)
- 开发语言：Rust（对 OrbbecSDK v2 C API 自建一层封装）
- 用途：抓取定位 / 商品识别 / VLA 模型输入

## 1. 硬件规格与连接

Gemini 335 是**主动/被动双目立体**相机（850nm 红外补光），输出 Depth / IR / RGB / 点云 / IMU。

| 项目 | 规格 |
|---|---|
| 深度分辨率/帧率 | 最高 1280×800 @ 30fps |
| RGB 分辨率/帧率 | 最高 1920×1080 @ 30fps |
| 深度 FOV | H90° V65° |
| RGB FOV | H86° V55° |
| 深度范围 | 0.10–20m+（最优 0.26–3m）|
| 精度 | ≤1.5% RMSE @2m |
| 深度技术 | 双目立体（主动+被动）|
| 接口 | USB 3.0 Type-C（供电+数据）|
| IMU / Trigger | 支持 / 支持 |
| 功耗 | 平均 <3.0W |

**关键约束**：必须接在 **USB 3.0（蓝色）口**，USB 2.0 口会无法识别或无法出图。

## 2. 总体架构

```
Gemini 335 (USB 3.0)
   │
   ├─ 深度流 1280×800@30 ──────────────┐
   ├─ RGB流  1920×1080@30 ──→ 商品识别 (PaddleOCR / 视觉模型)
   │                                   ├─ D2C 对齐 (Depth↔RGB)
   └─ IMU                              │
                                       ▼
                              ┌──────────────────┐
                              │  orbbec (Rust)    │  安全高层 API
                              │  ┌──────────────┐ │
                              │  │ orbbec-sys   │ │  bindgen FFI → libOrbbecSDK.so
                              │  └──────────────┘ │
                              └──────────────────┘
        ├─ 抓取定位: 对齐深度 (u,v) → 相机系 3D (fx,fy,cx,cy) → 手眼TF → 机械臂系
        ├─ VLA 输入:  RGB + 对齐深度 (带硬件时间戳)
        └─ 点云:      点云滤波 → 避障/建图/抓取
```

## 3. 底层 SDK：安装 OrbbecSDK v2（一次性）

Rust 封装只做 FFI 绑定，底层仍是官方 C++/C 库 `libOrbbecSDK.so`，需先编译安装。

### 3.1 系统依赖

```bash
sudo apt update
sudo apt install -y build-essential cmake git pkg-config \
  libusb-1.0-0-dev libgl1-mesa-dev libegl1-mesa-dev libgles2-mesa-dev \
  libglew-dev libopencv-dev libglog-dev
```

### 3.2 编译并安装 SDK

```bash
git clone --recursive https://github.com/orbbec/OrbbecSDK_v2.git
cd OrbbecSDK_v2 && mkdir build && cd build
cmake ..
make -j$(nproc)
sudo make install        # 头文件 → /usr/local/include，库 → /usr/local/lib
```

### 3.3 udev 权限规则（非 root 用户免 sudo 访问）

```bash
sudo cp ../scripts/99-orbec.rules /etc/udev/rules.d/
sudo udevadm control --reload-rules
sudo udevadm trigger
```

### 3.4 验证

- 插入相机（USB 3.0 口）后 `lsusb` 应看到 VID `2bc5` 的 Orbbec 设备。
- 用官方 GUI **Orbbec Viewer**（官网 SDK 下载页）确认 RGB / 深度 / IR 出图正常。
- `ldd` 确认库依赖完整，必要时 `export LD_LIBRARY_PATH=/usr/local/lib`。

## 4. Rust 封装层设计（本项目自建）

**仓库结构**（放于 `orbbec-rs/` 下，Cargo workspace）：

```
orbbec-rs/
├── Cargo.toml                # workspace
├── orbbec-sys/               # 低层：bindgen 生成的 FFI 绑定
│   ├── build.rs              # 查找已安装 SDK 并链接 libOrbbecSDK
│   ├── wrapper.h             # include <libobsensor/ObSensor.h>
│   └── src/lib.rs            # bindgen 输出（可手写精简绑定）
└── orbbec/                   # 高层：安全、惯用的 Rust API
    ├── src/
    │   ├── lib.rs
    │   ├── context.rs        # 设备枚举 / 创建
    │   ├── device.rs         # 设备信息、传感器列表
    │   ├── pipeline.rs       # 采集管线、帧同步
    │   ├── stream.rs         # StreamProfile / Config / 帧类型
    │   ├── frame.rs          # ColorFrame / DepthFrame / PointCloudFrame
    │   ├── align.rs          # D2C 对齐
    │   ├── camera.rs         # 内参 fx/fy/cx/cy、畸变、外参
    │   └── pointcloud.rs     # 点云生成与滤波
    └── examples/
        ├── color_depth_viewer.rs   # RGB+对齐深度预览
        ├── rgb_to_opencv.rs        # RGB 帧 → OpenCV Mat（给 PaddleOCR）
        └── pixel_to_3d.rs          # 像素→相机系 3D 点（抓取用）
```

### 4.1 链接方式（build.rs）

SDK 安装后位于 `/usr/local`。`orbbec-sys/build.rs`：

1. 依次从环境变量 `OB_SDK_ROOT`、默认 `/usr/local` 查找 SDK。
2. 输出链接参数：`cargo:rustc-link-lib=OrbbecSDK`、`cargo:rustc-link-search=/usr/local/lib`。
3. SDK 依赖的动态库（libusb / glog 等随 SDK 分发）通过 `LD_LIBRARY_PATH` 或 RPATH 指向对应目录。

> 备选：Rust 生态已有 `orbbec-sdk-sys`（低层，版本 0.1.2+2.5.5，从源码构建）和
> `orbbec-sdk`（高层，0.1.0，2025 年发布）。两者都很新、且不链接系统已装 SDK，
> 故本项目自建封装，兼容"装好 SDK 直接 Rust 开发"的目标；后续如发现其成熟，
> 可迁移复用其绑定，仅保留我们上层的应用逻辑。

### 4.2 安全高层 API 要点（对接 C 回调）

- `ob_*` 回调是 C 线程回调，`orbbec` 层需将 frame 指针包成 RAII 类型，
  通过 `Arc<Mutex>` / `crossbeam-channel` 发送给应用线程，避免回调内阻塞。
- 时间戳：保留 SDK 硬件时间戳（`getSystemTimeStamp` / 帧内置），供 VLA 对齐。
- D2C：开启 `enableFrameSync` + 设置对齐模式（depth→color 或 color→depth），
  由片上 MX6800 ASIC 完成，Rust 层仅取对齐结果。

## 5. 应用层功能设计

| 需求 | 方案 | 配置 |
|---|---|---|
| 抓取定位 | 深度对齐到 RGB → 像素 (u,v) 取深度 d → 用内参求 3D 点 (X,Y,Z) → 手眼外参变换到机械臂系 | 1280×800 深度 + D2C |
| 商品识别 | RGB 帧 (BGR) → OpenCV Mat → 送入 PaddleOCR | 1920×1080@30 |
| VLA 输入 | RGB + 对齐深度，成对带时间戳发布 | 同帧同步 |
| 避障/建图 | 深度 → 点云 → 滤波（去离群点）| 按需 30fps |

### 5.1 像素 → 3D（抓取核心）

使用 `ob_camera_param`（深度内参 fx, fy, cx, cy）：

```
X = (u - cx) * d / fx
Y = (v - cy) * d / fy
Z = d           # 单位需换算为米（深度值/1000 或按 SDK 单位缩放）
```

### 5.2 手眼标定

相机→机械臂外参（4×4 变换）用 **ArUco / easy_handeye** 一次性标定，
得到 `T_cam_to_base`，与 5.1 的相机系 3D 点相乘即得机械臂系坐标。

## 6. 与现有工程集成

| 工程 | 对接方式 |
|---|---|
| PaddleOCR | Rust 侧经 FFI 或共享内存把 RGB 帧转给 Python OCR 服务 |
| lingbot-vla-v2 | RGB-D 帧流（成对 + 时间戳）作为模型观测输入 |
| lingbot-map | 深度/点云订阅用于建图与避障 |

推荐 Rust 采集节点作为**独立进程**，通过 ZeroMQ / 共享内存对外发布
（RGB、对齐深度、点云、内参+外参），其余工程各自订阅，解耦语言栈。

## 7. 里程碑

1. SDK 编译安装 + udev + OrbbecViewer 出图验证（0.5 天）
2. `orbbec-sys` 绑定 + `orbbec` 高层：枚举设备、单流出图（1 天）
3. RGB + 深度同步 + D2C 对齐 + 内参读取（1 天）
4. 像素→3D + 点云输出 + 手眼标定闭环（1–2 天）
5. RGB 送 PaddleOCR、RGB-D 时间戳流对接 VLA（1 天）

## 8. 风险与备选

- **USB 带宽**：1920×1080 RGB + 1280×800 深度同时开启需稳定 USB3.0，建议独占控制器。
- **Rust crate 成熟度**：若自建绑定工作量大，可改用 `orbbec-sdk` 现成 crate 起步。
- **多相机同步**：330 系列支持硬件 Trigger，后续扩展需在 `orbbec` 层加多设备接口。
- **SDK 版本**：Gemini 335 需 OrbbecSDK v2（≥2.3 推荐，当前最新 2.5.x）。
