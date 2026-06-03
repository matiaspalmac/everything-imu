#---------------------------------------------------------------------------------
# everything-imu 3DS forwarder — devkitARM / libctru build
#---------------------------------------------------------------------------------
.SUFFIXES:

ifeq ($(strip $(DEVKITARM)),)
$(error "Set DEVKITARM in your environment. e.g. export DEVKITARM=/opt/devkitpro/devkitARM")
endif

TOPDIR ?= $(CURDIR)
include $(DEVKITARM)/3ds_rules

#---------------------------------------------------------------------------------
TARGET		:=	eimu-3ds
BUILD		:=	build
SOURCES		:=	source
INCLUDES	:=	include

APP_TITLE	:=	everything-imu 3DS
APP_DESCRIPTION	:=	IMU forwarder for everything-imu
APP_AUTHOR	:=	everything-imu

#---------------------------------------------------------------------------------
ARCH	:=	-march=armv6k -mtune=mpcore -mfloat-abi=hard -mtp=soft

CFLAGS	:=	-g -Wall -O2 -mword-relocations \
		-ffunction-sections \
		$(ARCH)
CFLAGS	+=	$(INCLUDE) -D__3DS__
CXXFLAGS	:=	$(CFLAGS) -fno-rtti -fno-exceptions -std=gnu++11
ASFLAGS	:=	-g $(ARCH)
LDFLAGS	=	-specs=3dsx.specs -g $(ARCH) -Wl,-Map,$(notdir $*.map)

LIBS	:=	-lctru -lm
LIBDIRS	:=	$(CTRULIB)

#---------------------------------------------------------------------------------
ifneq ($(BUILD),$(notdir $(CURDIR)))
#---------------------------------------------------------------------------------
export OUTPUT	:=	$(CURDIR)/$(TARGET)
export TOPDIR	:=	$(CURDIR)

export VPATH	:=	$(foreach dir,$(SOURCES),$(CURDIR)/$(dir))
export DEPSDIR	:=	$(CURDIR)/$(BUILD)

CFILES		:=	$(foreach dir,$(SOURCES),$(notdir $(wildcard $(dir)/*.c)))
CPPFILES	:=	$(foreach dir,$(SOURCES),$(notdir $(wildcard $(dir)/*.cpp)))
SFILES		:=	$(foreach dir,$(SOURCES),$(notdir $(wildcard $(dir)/*.s)))

ifeq ($(strip $(CPPFILES)),)
	export LD	:=	$(CC)
else
	export LD	:=	$(CXX)
endif

export OFILES	:=	$(CPPFILES:.cpp=.o) $(CFILES:.c=.o) $(SFILES:.s=.o)

export INCLUDE	:=	$(foreach dir,$(INCLUDES),-I$(CURDIR)/$(dir)) \
			$(foreach dir,$(LIBDIRS),-I$(dir)/include) \
			-I$(CURDIR)/$(BUILD)

export LIBPATHS	:=	$(foreach dir,$(LIBDIRS),-L$(dir)/lib)

.PHONY: $(BUILD) clean all

all: $(BUILD)

$(BUILD):
	@[ -d $@ ] || mkdir -p $@
	@$(MAKE) --no-print-directory -C $(BUILD) -f $(CURDIR)/Makefile

clean:
	@echo clean ...
	@rm -fr $(BUILD) $(TARGET).3dsx $(TARGET).elf $(TARGET).smdh

#---------------------------------------------------------------------------------
else
#---------------------------------------------------------------------------------
DEPENDS	:=	$(OFILES:.o=.d)

$(OUTPUT).3dsx	:	$(OUTPUT).elf
$(OUTPUT).elf	:	$(OFILES)

-include $(DEPENDS)

#---------------------------------------------------------------------------------
endif
#---------------------------------------------------------------------------------
