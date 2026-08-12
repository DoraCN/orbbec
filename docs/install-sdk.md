# 安装奥比中光官方 SDK (OrbbecSDK v2)

> 适用：所有支持 OrbbecSDK v2 的奥比中光相机
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
> 否则需按第 6 节调整 `OB_SDK_ROOT`。

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

> 注意：源码编译**不含** `OrbbecViewer`（GUI 调试工具）。它只在官网预编译包里提供，
> 需要的话请用方式二下载，或直接用 `bin/` 下的命令行示例验证出图。

---

## 3. 方式二：官方预编译 SDK（快速验证用）

1. 访问官网 SDK 下载页 <https://www.orbbec.com/developers/orbbec-sdk/>
   选择 **Linux x86_64** 版本（当前最新 v2.5.x）。
2. 解压后目录通常包含：`bin/`、`include/`、`lib/`、`OrbbecViewer/`。
3. 把解压目录移动到统一前缀（与方式一保持一致）：
   `sudo mv <解压目录> /opt/OrbbecSDK`
   然后按第 6 节设置环境变量。
4. GUI 工具 `OrbbecViewer` 在预编译包的 `OrbbecViewer/` 目录下，
   可直接运行：`./OrbbecViewer`（源码编译版没有它）

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

**GUI 验证**：源码编译版没有 `OrbbecViewer`，可用官方预编译包里的 GUI 工具，
或直接用命令行示例验证出图（如 `/opt/OrbbecSDK/bin/ob_depth` 打印深度数据、
`ob_enumerate` 枚举设备）。确认 RGB / 深度 / IR 三路数据正常、深度值随距离正确变化。

---

## 6. 构建 Rust 封装（cargo build）

### 6.1 环境变量设置（编译与运行前，必做）

编译本仓库前，需在**当前终端**设置两个环境变量（本仓库全部按 `/opt/OrbbecSDK`
安装，只需在下面这一处设置，无需再配置其他变量）：

```bash
# 编译用：告诉 orbbec-sys 到 /opt/OrbbecSDK 找头文件与 libOrbbecSDK.so（bindgen + 链接）
export OB_SDK_ROOT=/opt/OrbbecSDK

# 运行用：告诉动态链接器运行时到哪里加载 libOrbbecSDK.so
export LD_LIBRARY_PATH=/opt/OrbbecSDK/lib:$LD_LIBRARY_PATH

# 可选：写入 ~/.bashrc，新开终端自动生效
echo 'export OB_SDK_ROOT=/opt/OrbbecSDK' >> ~/.bashrc
echo 'export LD_LIBRARY_PATH=/opt/OrbbecSDK/lib:$LD_LIBRARY_PATH' >> ~/.bashrc
```

说明：

- `OB_SDK_ROOT`：`orbbec-sys/build.rs` 用它定位
  `include/libobsensor/ObSensor.h` 与 `lib/libOrbbecSDK.so`，用于 bindgen 与链接
  （构建期）。未设置时默认找 `/usr/local`。
- `LD_LIBRARY_PATH`：程序**运行时**加载 `libOrbbecSDK.so` 需要。库自带的依赖
  已通过自身 rpath（`$ORIGIN`）解析；`lib/extensions/` 插件由 SDK 运行时按相对
  路径加载，若报缺 `libdepthengine.so` 等，再追加其目录即可：
  `export LD_LIBRARY_PATH=/opt/OrbbecSDK/lib/extensions/depthengine:$LD_LIBRARY_PATH`

### 6.2 编译

```bash
# 在 orbbec 项目根目录，先确保 6.1 的两个环境变量已设置
cargo build --release
```

构建期 `orbbec-sys/build.rs` 会：

- 运行 bindgen 生成 Rust 绑定（需要 clang，见第 1 节）；
- 输出链接参数 `-lOrbbecSDK` 与库搜索路径。

### 6.3 运行

```bash
# 新开终端时先重新设置环境变量（或已写入 ~/.bashrc 则执行 source ~/.bashrc）
cargo run --release
```

> 若报错 `OrbbecSDK v2 headers not found`，说明 `OB_SDK_ROOT` 未设置或路径不对，
> 检查 6.1；若报 `error while loading shared libraries`，说明 `LD_LIBRARY_PATH`
> 未生效，回到 6.1。

---

## 7. 常见问题

| 现象 | 原因 | 解决 |
|---|---|---|
| `lsusb` 看不到 2bc5 设备 | 插在 USB2.0 口 / 线缆问题 | 换 USB3.0 口或数据线 |
| 能识别但没图像 | udev 权限 | 执行第 4 节，重插相机 |
| Viewer 报找不到 libusb | SDK 运行依赖缺失 | `sudo apt install libusb-1.0-0` |
| `cargo build` 报 bindgen 找不到 libclang | 未装 clang | `sudo apt install clang libclang-dev` |
| `cargo build` 报 `headers not found` | 未设 `OB_SDK_ROOT` | `export OB_SDK_ROOT=/opt/OrbbecSDK`（见 6.1 节） |
| 运行时报 `error while loading shared libraries` | 找不到 `libOrbbecSDK.so` | 设置 `LD_LIBRARY_PATH=/opt/OrbbecSDK/lib`（见 6.1 节） |
| 运行时报缺 `libdepthengine.so` 等扩展 | 找不到 `lib/extensions/` 插件 | 将 `extensions/depthengine` 等目录加入 `LD_LIBRARY_PATH`（见 6.1 节） |
| 相机无法枚举流配置 | SDK 版本过旧 | 升级到 v2.3+（推荐 v2.5.x） |

---

## 8. 相关资源

- 官方源码：<https://github.com/orbbec/OrbbecSDK_v2>
- SDK 构建教程：`OrbbecSDK_v2/docs/tutorial/building_orbbec_sdk.md`
- 官网 SDK 下载：<https://www.orbbec.com/developers/orbbec-sdk/>
- 官方文档：<https://doc.orbbec.com/>
- 社区论坛：<https://3dclub.orbbec3d.com/>
