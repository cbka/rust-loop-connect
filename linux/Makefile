# SPDX-License-Identifier: GPL-2.0-or-later
KVER ?= $(shell uname -r)
KDIR ?= /lib/modules/$(KVER)/build

obj-m := ek-loop-connect.o

.PHONY: all clean install check
all:
	$(MAKE) -C $(KDIR) M=$(CURDIR) modules
clean:
	$(MAKE) -C $(KDIR) M=$(CURDIR) clean
install:
	$(MAKE) -C $(KDIR) M=$(CURDIR) modules_install
	depmod -a $(KVER)

check:
	$(CC) -std=c11 -Wall -Wextra -Werror -o .response-validation-test test-response-validation.c
	./.response-validation-test
	rm -f .response-validation-test
