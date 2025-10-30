default: build

build_docker:
	docker build --target base -t birnam_base .
	docker build --target tcc -t birnam_tcc .
	docker build --target birnam0 -t birnam .

build: libid system vendored_libs

libid:
	tcc -fPIC -shared -rdynamic -g \
		-I/opt/homebrew/include \
		-DPREFIX='"/usr/local/lib/"' \
		-DSYSARCH='"aarch64"' \
		-DSYSOS='"darwin"' \
		src/libid.c \
		-L/opt/homebrew/lib -lgc \
		-o bin/libid.so

system:
	tcc 	-L/usr/lib/ \
	 	-rdynamic \
		-I /usr/local/include/ \
		-I/opt/homebrew/include \
		-L /usr/lib \
		-L /usr/local/lib -ldl -ltcc -lreadline -g \
		src/sys/system.c
	mv a.out bin/system

build_libelf:
	cd vendor/libelf
	./configure
	make
	cd -

hello_module:
	tcc -fPIC -shared -rdynamic -g \
		-I/opt/homebrew/include \
		src/hello_module.c \
		-o bin/hello_module.so

vendored_libs:
	tcc -fPIC -shared -rdynamic -soname=packcc -Wl -o bin/packcc.so vendor/PackCC/src/packcc.c
        # tcc -rdynamic -shared -soname=libelf -o libelf.so vendor/

obj:
	tcc -fPIC -shared -rdynamic -Wl -g src/obj.c
	mv a.out bin/obj.so
run:
	docker run --rm -it birnam /app/system

shell:
	docker run --rm -it birnam bash

dcom:
	docker-compose up -d
	docker-compose run birnam bash
