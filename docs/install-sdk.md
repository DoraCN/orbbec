# 安装奥比中光官方 SDK (OrbbecSDK v2)

> 适用相机：Gemini 335 / 330 系列
> 适用系统：Ubuntu 22.04 x86_64
> 说明：本仓库的 Rust 封装通过 FFI 链接**系统安装的 `libOrbbecSDK.so`**，
> 因此必须先按本文安装官方 SDK，才能 `cargo build` 本仓库。

---

## 1. 前置系统依赖

```bash
sudo apt update
sudo apt install -y \
  build-essential git cmake pkg-config \
  libusb-1.0-0-dev libglog-dev libopencv-dev \
  libgl1-mesa-dev libegl1-mesa-dev libgles2-mesa-dev libglew-dev \
  clang libclang-dev
```

说明：

- `build-essential git cmake`：编译 SDK 源码所需。
- `libusb-1.0-0-dev libglog-dev`：SDK 运行依赖。
- `libopencv-dev` + OpenGL 相关：编译 SDK 自带 examples / 运行官方 Viewer 需要。
- **`clang libclang-dev`：`orbbec-sys` 用 bindgen 生成 Rust 绑定，必须装**，否则 `cargo build` 会在 bindgen 阶段报错。
- 确认版本：`cmake --version`（需 ≥3.15，22.04 自带 3.22 满足）。

---

## 2. 方式一：源码编译安装（推荐，与 Rust 封装配套）

### 2.1 克隆源码（含子模块）

```bash
git clone --recursive https://github.com/orbbec/OrbbecSDK_v2.git
cd OrbbecSDK_v2
```

> 第三方依赖（libusb、glog 等）通过 git submodule 引入，务必加 `--recursive`。
> 若漏拉了子模块：`git submodule update --init --recursive`

### 2.2 编译

```bash
mkdir build && cd build
cmake .. -DCMAKE_BUILD_TYPE=Release
make -j$(nproc)
```

### 2.3 安装

```bash
sudo make install
```

安装结果（默认前缀 `/usr/local`）：

| 内容 | 路径 |
|---|---|
| 头文件 | `/usr/local/include/libobsensor/*.h` |
| 动态库 | `/usr/local/lib/libOrbbecSDK.so*` |
| 配套依赖库 | `/usr/local/lib/` 下随 SDK 分发的 `.so` |

> 如需自定义前缀：`cmake .. -DCMAKE_INSTALL_PREFIX=/opt/orbbec`，
> 之后设置环境变量 `export OB_SDK_ROOT=/opt/orbbec` 供 Rust 构建时查找。

---

## 3. 方式二：官方预编译 SDK（快速验证用）

1. 访问官网 SDK 下载页 <https://www.orbbec.com/developers/orbbec-sdk/>
   选择 **Linux x86_64** 版本（当前最新 v2.5.x）。
2. 解压后目录通常包含：`bin/`、`include/`、`lib/`、`OrbbecViewer/`。
3. 用 `sudo make install` 或手动把 `include`、`lib` 拷贝到 `/usr/local`，
   或直接设置 `OB_SDK_ROOT=<解压目录>` 供 Rust 构建使用。
4. `OrbbecViewer`（GUI 调试工具）可直接运行：`./OrbbecViewer`

---

## 4. udev 权限规则（非 root 免权限访问相机）

### 方式一：源码仓库自带规则

```bash
sudo cp /path/to/OrbbecSDK_v2/scripts/99-orbec.rules /etc/udev/rules.d/
```

### 方式二：手动创建

```bash
sudo tee /etc/udev/rules.d/99-orbec.rules > /dev/null <<'EOF'
SUBSYSTEM=="usb", ATTRS{idVendor}=="2bc5", MODE:="0666", GROUP="plugdev"
SUBSYSTEM=="usb", ATTRS{idVendor}=="2bc5", MODE:="0666"
EOF
```

生效：

```bash
sudo udevadm control --reload-rules
sudo udevadm trigger
```

> 相机必须插在 **USB 3.0（蓝色）口**，USB 2.0 无法识别或无法出图。
> 重插相机后再执行验证。

---

## 5. 验证安装

```bash
# 1) 设备可见（VID 2bc5 = Orbbec）
lsusb | grep -i 2bc5

# 2) 库文件存在
ls -l /usr/local/lib/libOrbbecSDK.so*

# 3) 头文件存在
ls /usr/local/include/libobsensor/ObSensor.h

# 4) 动态库依赖完整（无 "not found"）
export LD_LIBRARY_PATH=/usr/local/lib:$LD_LIBRARY_PATH
ldd /usr/local/lib/libOrbbecSDK.so | grep -i "not found" || echo "依赖完整"
```

**GUI 验证**：运行 `OrbbecViewer`，确认 RGB / 深度 / IR 三路出图正常、
深度图随距离正确变化。

---

## 6. Rust 构建时的链接约定

`orbbec-sys/build.rs` 按以下顺序查找 SDK：

1. 环境变量 `OB_SDK_ROOT`（若设置，使用其下的 `include/` 与 `lib/`）；
2. 默认 `/usr/local`。

找到头文件 `include/libobsensor/ObSensor.h` 后：

- 运行 bindgen 生成 Rust 绑定；
- 输出链接参数：`-lOrbbecSDK` + 链接搜索路径。

构建：

```bash
# 在 orbbec 项目根目录
cargo build
```

运行编译出的程序前，确保运行时能找到动态库：

```bash
export LD_LIBRARY_PATH=/usr/local/lib:$LD_LIBRARY_PATH
```

> 若报错 `OrbbecSDK v2 头文件未找到`，说明尚未安装 SDK，回到第 2/3 节。

---

## 7. 常见问题

| 现象 | 原因 | 解决 |
|---|---|---|
| `lsusb` 看不到 2bc5 设备 | 插在 USB2.0 口 / 线缆问题 | 换 USB3.0 口或数据线 |
| 能识别但没图像 | udev 权限 | 执行第 4 节，重插相机 |
| Viewer 报找不到 libusb | SDK 运行依赖缺失 | `sudo apt install libusb-1.0-0` |
| `cargo build` 报 bindgen 找不到 libclang | 未装 clang | `sudo apt install clang libclang-dev` |
| 运行时报 `error while loading shared libraries` | 找不到 `libOrbbecSDK.so` | 设置 `LD_LIBRARY_PATH` |
| Gemini 335 无法枚举流配置 | SDK 版本过旧 | 升级到 v2.3+（推荐 v2.5.x） |

---

## 8. 相关资源

- 官方源码：<https://github.com/orbbec/OrbbecSDK_v2>
- SDK 构建教程：`OrbbecSDK_v2/docs/tutorial/building_orbbec_sdk.md`
- 官网 SDK 下载：<https://www.orbbec.com/developers/orbbec-sdk/>
- 官方文档：<https://doc.orbbec.com/>
- 社区论坛：<https://3dclub.orbbec3d.com/>
