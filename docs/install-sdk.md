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
  libusb-1.0-0-dev libgoogle-glog-dev libopencv-dev \
  libgl1-mesa-dev libegl1-mesa-dev libgles2-mesa-dev libglew-dev \
  clang libclang-dev
```

说明：

- `build-essential git cmake`：编译 SDK 源码所需。
- `libusb-1.0-0-dev libgoogle-glog-dev`：SDK 运行依赖（包名为 `libgoogle-glog-dev`，不是 `libglog-dev`）。
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
cmake .. -DCMAKE_BUILD_TYPE=Release -DCMAKE_INSTALL_PREFIX=/opt/OrbbecSDK
make -j$(nproc)
```

> 建议安装前缀统一用 `/opt/OrbbecSDK`（与官方预编译包目录一致），
> 否则需要手动设置 `OB_SDK_ROOT`，见下方 2.4 节。

### 2.3 安装

```bash
sudo make install
```

安装结果（本仓库配套前缀 `/opt/OrbbecSDK`）：

| 内容 | 路径 |
|---|---|
| 头文件 | `/opt/OrbbecSDK/include/libobsensor/*.h` |
| 动态库 | `/opt/OrbbecSDK/lib/libOrbbecSDK.so*` |
| 扩展插件库 | `/opt/OrbbecSDK/lib/extensions/`（`filters`、`depthengine`、`frameprocessor` 等） |
| 示例二进制 | `/opt/OrbbecSDK/bin/`（`ob_enumerate`、`ob_depth`、`ob_color` 等） |
| 配置文件 | `/opt/OrbbecSDK/lib/OrbbecSDKConfig.xml` |
| udev 规则与安装脚本 | `/opt/OrbbecSDK/shared/99-obsensor-libusb.rules`、`install_udev_rules.sh` |

### 2.4 设置 SDK 路径（Rust 构建用）

Rust 的 `orbbec-sys` 通过环境变量 `OB_SDK_ROOT` 定位 SDK，把它写入 shell 配置：

```bash
echo 'export OB_SDK_ROOT=/opt/OrbbecSDK' >> ~/.bashrc
source ~/.bashrc
```

> 注意：`libOrbbecSDK.so` 运行时还要找到 `lib/extensions/` 下的插件库
> （它们依赖同一前缀，若装在 `/opt/OrbbecSDK` 则相对路径 `$ORIGIN/../lib` 已自动可解析）。
> 若报缺少 `libdepthengine.so` 等，显式设置：
> `export LD_LIBRARY_PATH=/opt/OrbbecSDK/lib:/opt/OrbbecSDK/lib/extensions/depthengine:$LD_LIBRARY_PATH`

---

## 3. 方式二：官方预编译 SDK（快速验证用）

1. 访问官网 SDK 下载页 <https://www.orbbec.com/developers/orbbec-sdk/>
   选择 **Linux x86_64** 版本（当前最新 v2.5.x）。
2. 解压后目录通常包含：`bin/`、`include/`、`lib/`、`OrbbecViewer/`。
3. 把解压目录移动到统一前缀（与方式一保持一致）：
   `sudo mv <解压目录> /opt/OrbbecSDK`
   然后设置 `export OB_SDK_ROOT=/opt/OrbbecSDK`（见 2.4 节）。
4. `OrbbecViewer`（GUI 调试工具）可直接运行：`/opt/OrbbecSDK/OrbbecViewer/OrbbecViewer`

---

## 4. udev 权限规则（非 root 免权限访问相机）

### 方式一：源码仓库自带规则

```bash
sudo cp /opt/OrbbecSDK/shared/99-obsensor-libusb.rules /etc/udev/rules.d/
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

> SDK 还自带一键安装脚本 `/opt/OrbbecSDK/install_udev_rules.sh` 与
> 环境配置脚本 `/opt/OrbbecSDK/setup.sh`（后者可设置 `OB_SDK_ROOT`/`LD_LIBRARY_PATH`）。
>
> 相机必须插在 **USB 3.0（蓝色）口**，USB 2.0 无法识别或无法出图。
> 重插相机后再执行验证。

---

## 5. 验证安装

```bash
# 1) 设备可见（VID 2bc5 = Orbbec）
lsusb | grep -i 2bc5

# 2) 库文件存在
ls -l /opt/OrbbecSDK/lib/libOrbbecSDK.so*

# 3) 头文件存在
ls /opt/OrbbecSDK/include/libobsensor/ObSensor.h

# 4) 动态库依赖完整（无 "not found"）
export LD_LIBRARY_PATH=/opt/OrbbecSDK/lib:$LD_LIBRARY_PATH
ldd /opt/OrbbecSDK/lib/libOrbbecSDK.so | grep -i "not found" || echo "依赖完整"

# 5) 示例程序可运行（枚举设备）
/opt/OrbbecSDK/bin/ob_enumerate
```

**GUI 验证**：运行 `OrbbecViewer`（`/opt/OrbbecSDK/OrbbecViewer/OrbbecViewer`），
确认 RGB / 深度 / IR 三路出图正常、深度图随距离正确变化。

---

## 6. Rust 构建时的链接约定

`orbbec-sys/build.rs` 按以下顺序查找 SDK：

1. 环境变量 `OB_SDK_ROOT`（若设置，使用其下的 `include/` 与 `lib/`）；
2. 默认 `/usr/local`。

> 本仓库按 `/opt/OrbbecSDK` 安装，因此**必须先 `export OB_SDK_ROOT=/opt/OrbbecSDK`**
> （见 2.4 节），否则构建会在 bindgen 前报 `OrbbecSDK v2 headers not found`。

找到头文件 `include/libobsensor/ObSensor.h` 后：

- 运行 bindgen 生成 Rust 绑定；
- 输出链接参数：`-lOrbbecSDK` + 链接搜索路径。

构建：

```bash
# 在 orbbec 项目根目录（需已设置 OB_SDK_ROOT）
cargo build
```

运行编译出的程序前，确保运行时能找到动态库与扩展插件：

```bash
export LD_LIBRARY_PATH=/opt/OrbbecSDK/lib:$LD_LIBRARY_PATH
```

> 若报错 `OrbbecSDK v2 头文件未找到`，说明尚未安装 SDK 或未设置
> `OB_SDK_ROOT`，回到第 2/3 节。

---

## 7. 常见问题

| 现象 | 原因 | 解决 |
|---|---|---|
| `lsusb` 看不到 2bc5 设备 | 插在 USB2.0 口 / 线缆问题 | 换 USB3.0 口或数据线 |
| 能识别但没图像 | udev 权限 | 执行第 4 节，重插相机 |
| Viewer 报找不到 libusb | SDK 运行依赖缺失 | `sudo apt install libusb-1.0-0` |
| `cargo build` 报 bindgen 找不到 libclang | 未装 clang | `sudo apt install clang libclang-dev` |
| `cargo build` 报 `headers not found` | 未设 `OB_SDK_ROOT` | `export OB_SDK_ROOT=/opt/OrbbecSDK`（见 2.4 节） |
| 运行时报 `error while loading shared libraries` | 找不到 `libOrbbecSDK.so` | 设置 `LD_LIBRARY_PATH=/opt/OrbbecSDK/lib` |
| 运行时报缺 `libdepthengine.so` 等扩展 | 找不到 `lib/extensions/` 插件 | 将 `extensions/depthengine` 等目录加入 `LD_LIBRARY_PATH` |
| Gemini 335 无法枚举流配置 | SDK 版本过旧 | 升级到 v2.3+（推荐 v2.5.x） |

---

## 8. 相关资源

- 官方源码：<https://github.com/orbbec/OrbbecSDK_v2>
- SDK 构建教程：`OrbbecSDK_v2/docs/tutorial/building_orbbec_sdk.md`
- 官网 SDK 下载：<https://www.orbbec.com/developers/orbbec-sdk/>
- 官方文档：<https://doc.orbbec.com/>
- 社区论坛：<https://3dclub.orbbec3d.com/>
