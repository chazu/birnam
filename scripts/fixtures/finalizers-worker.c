/* Minimal transport peer for the native Isolate handle lifecycle oracle.
   Actual image-backed worker behavior is checked separately. */
#include <assert.h>
#include <stdint.h>
#include <stdio.h>
#include <stdlib.h>
#include <unistd.h>
int main(int argc,char **argv) {
    assert(argc==7);int fd=atoi(argv[6]);assert(fd>0);
    FILE *receipt=fopen(getenv("FINALIZER_WORKER_PID_FILE"),"w");assert(receipt);
    fprintf(receipt,"%d\n",getpid());fclose(receipt);
    const uint8_t ready[20]={'B','I','S','O',0,1,1,0};assert(write(fd,ready,sizeof(ready))==sizeof(ready));
    for(;;) pause();
}
