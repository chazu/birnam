reboot_id:
	tcc 	-L/usr/lib/x86_64-linux-gnu/ \
	 	-rdynamic \
		-I ./vendor/ \
		-I ./vendor/tinycc \
		-L /usr/lib \
		-L /usr/local/lib -ldl -ltcc -g \
		-o bin/id \
		src/id.c \

reboot_peg:
	tcc 	-L/usr/lib/x86_64-linux-gnu/ \
	 	-rdynamic \
		-I ./vendor/ \
		-I ./vendor/tinycc \
		-L /usr/lib \
		-L /usr/local/lib -ldl -ltcc -g \
		-o bin/peg \
		src/peg.c \

reboot: reboot_id reboot_peg
	tcc 	-L/usr/lib/x86_64-linux-gnu/ \
	 	-rdynamic \
		-I ./vendor/ \
		-I ./vendor/tinycc \
		-L /usr/lib \
		-L /usr/local/lib -ldl -ltcc -ldynload -lreadline -g \
		-o bin/brn \
		src/main.c -vv


default: reboot

build_libelf:
	cd vendor/libelf
	./configure
	make
	cd -

build_vendored_libs:
	tcc -fPIC -shared -rdynamic -soname=packcc -Wl -o bin/packcc.so vendor/PackCC/src/packcc.c
        # tcc -rdynamic -shared -soname=libelf -o libelf.so vendor/

build_obj:
	tcc -fPIC -shared -rdynamic -Wl -g src/obj.c
	mv a.out bin/obj.so
run:
	docker run --rm -it birnam /app/system

shell:
	docker run --rm -it birnam bash

dcom:
	docker-compose up -d
	docker-compose run birnam bash
