default: build

build_docker:
	docker build --target base -t birnam_base .
	docker build --target tcc -t birnam_tcc .
	docker build --target birnam0 -t birnam .

build: obj system vendored_libs

system:
	tcc 	-L/usr/lib/ \
	 	-rdynamic \
		-I /usr/local/include/ \
		-L /usr/lib \
		-L /usr/local/lib -ldl -ltcc -lreadline -g \
		src/sys/system.c -vv
	mv a.out bin/system

build_libelf:
	cd vendor/libelf
	./configure
	make
	cd -

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
