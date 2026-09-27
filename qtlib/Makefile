ifeq ($(shell uname),Darwin)
    NATIVE_LIBS := -framework Security -framework CoreFoundation -liconv -lresolv
else
    NATIVE_LIBS := -lpthread -ldl -lm
endif

.PHONY: all clean
all:
	cargo build --lib -p qtlib
	$(CXX) src/main.cpp -L../target/debug -lpiratepaperrust $(NATIVE_LIBS) -o run
	./run

clean:
	cargo clean -p qtlib
	rm -f run
