# 指定内核源码目录（替换为你的内核源码路径）
KERNELDIR ?= /lib/modules/$(shell uname -r)/build
PWD := $(shell pwd)

# 模块名称
MODULE_NAME = rust_kv

# Rust 内核模块编译规则
obj-m += $(MODULE_NAME).o
$(MODULE_NAME)-y := kv_module.o

# 内核编译标志（启用 Rust 支持）
EXTRA_CFLAGS += -Wall -Wextra
RUSTFLAGS += --cfg kernel

# 编译目标
all:
	$(MAKE) -C $(KERNELDIR) M=$(PWD) modules

# 清理目标
clean:
	$(MAKE) -C $(KERNELDIR) M=$(PWD) clean