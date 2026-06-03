cmake_minimum_required(VERSION 3.10)

if(NOT DEFINED CMAKE_TOOLCHAIN_FILE)
	if(DEFINED ENV{VITASDK})
		set(CMAKE_TOOLCHAIN_FILE "$ENV{VITASDK}/share/vita.cmake" CACHE PATH "toolchain file")
	else()
		message(FATAL_ERROR "Set the VITASDK environment variable, e.g. export VITASDK=/usr/local/vitasdk")
	endif()
endif()

project(eimu-vita C)

include("${VITASDK}/share/vita.cmake" REQUIRED)

set(VITA_APP_NAME "everything-imu Vita")
set(VITA_TITLEID  "EIMU09306")
set(VITA_VERSION  "01.00")

add_executable(${PROJECT_NAME}
	src/main.c
)

target_link_libraries(${PROJECT_NAME}
	SceMotion_stub
	SceNet_stub
	SceNetCtl_stub
	SceCtrl_stub
	SceSysmodule_stub
)

vita_create_self(${PROJECT_NAME}.self ${PROJECT_NAME})
vita_create_vpk(${PROJECT_NAME}.vpk ${VITA_TITLEID} ${PROJECT_NAME}.self
	VERSION ${VITA_VERSION}
	NAME ${VITA_APP_NAME}
)
