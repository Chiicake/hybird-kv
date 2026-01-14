KERNELDIR ?= /lib/modules/$(shell uname -r)/build
PWD := $(shell pwd)

MODULE_NAME = rust_kv

obj-m += $(MODULE_NAME).o
$(MODULE_NAME)-y := kv_module.o

EXTRA_CFLAGS += -Wall -Wextra
RUSTFLAGS += --cfg kernel

all:
	$(MAKE) -C $(KERNELDIR) M=$(PWD) modules

clean:
	$(MAKE) -C $(KERNELDIR) M=$(PWD) clean