default: build_docker

build_docker:
	docker build --target base -t birnam_base .
	docker build --target tcc -t birnam_tcc .
	docker build --target birnam0 -t birnam .

build: build_toy build_siphon build_vendored_libs

build_siphon:
	tcc -L/usr/lib/x86_64-linux-gnu/ \
		-L /usr/local/lib -ldl -lreadline src/siphon.c
	mv a.out bin/siphon

build_libelf:
	cd vendor/libelf
	./configure
	make
	cd -

build_vendored_libs:
	tcc -rdynamic -shared -soname=packcc -o packcc.so vendor/PackCC/src/packcc.c

        # tcc -rdynamic -shared -soname=libelf -o libelf.so vendor/

build_toy:
	tcc -shared src/toy.c
	mv a.out bin/toy.so
run:
	docker run --rm -it birnam /app/siphon

shell:
	docker run --rm -it birnam bash

dcom:
	docker-compose up -d
	docker-compose run birnam bash
