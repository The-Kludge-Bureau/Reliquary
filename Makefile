TRIPLE   = i686-pc-windows-msvc
XWIN_SDK = $(CURDIR)/xwinSDK

export CC_i686_pc_windows_msvc     := clang-cl
export CFLAGS_i686_pc_windows_msvc := \
    -imsvc $(XWIN_SDK)/crt/include \
    -imsvc $(XWIN_SDK)/sdk/include/ucrt \
    -imsvc $(XWIN_SDK)/sdk/include/um \
    -imsvc $(XWIN_SDK)/sdk/include/shared
export RUSTFLAGS := \
    -Clinker-flavor=msvc \
    -Lnative=$(XWIN_SDK)/crt/lib/x86 \
    -Lnative=$(XWIN_SDK)/sdk/lib/ucrt/x86 \
    -Lnative=$(XWIN_SDK)/sdk/lib/um/x86

.PHONY: all release debug check clean

all: release

release:
	cargo build --release --target $(TRIPLE)

debug:
	cargo build --target $(TRIPLE)

check:
	cargo check --target $(TRIPLE)

clean:
	cargo clean
