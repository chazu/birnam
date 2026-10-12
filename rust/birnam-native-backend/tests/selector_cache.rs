//! Exercise generated C with repeated spellings at distinct addresses.
use std::{fs, process::Command};

#[test]
fn duplicate_selector_literals_do_not_fill_the_cache_or_cache_dispatch() {
    let dir = std::env::temp_dir().join(format!("birnam-selector-cache-{}", std::process::id()));
    fs::create_dir_all(&dir).unwrap();
    let source = dir.join("cache.c");
    let binary = dir.join("cache");
    let mut c = include_str!("../src/native_prelude.c").to_owned();
    c.push_str(r#"
#include <assert.h>
static char repeated[700][32];
static char names[520][32];
static size_t interns, sends, verdict;
static int fail_owner, reenter_owner, reentered;
static R message(void*ctx,O receiver,O sel,const A*a,size_t n){
 (void)ctx;(void)n;
 if(sel==id.intern){
  interns++;
  if(!strcmp((const char*)a[0].data.bytes.pointer,"retain:"))return ok(obj((O)2));
  if(!strcmp((const char*)a[0].data.bytes.pointer,"release:"))return ok(obj((O)3));
  const char*name=(const char*)a[0].data.bytes.pointer;
  if(!strncmp(name,"unique-",7))return ok(obj((O)(1000+strtoul(name+7,0,10))));
  return ok(obj((O)4));
 }
 if(receiver==roots)return ok(word(1));
 if(receiver==owner){
  if(reenter_owner&&!reentered){
   reentered=1;Call nested={0};assert(selector(&nested,"unique-519")== (O)1519);
   assert(!finish(&nested,obj(owner)).status);
  }
  if(fail_owner){R error=bad();error.detail=705;return error;}
  return ok(obj(owner));
 }
 sends++;return ok(word(verdict));
}
int main(void){
 id.message=message;id.receiver=(O)10;id.intern=(O)11;roots=(O)12;owner=(O)13;cache_selectors=1;
 Call c={0};
 for(size_t i=0;i<700;i++){strcpy(repeated[i],"same-selector");assert(selector(&c,repeated[i])==(O)4);}
 assert(interns==2); /* one spelling plus Roots retain: */
 verdict=41;assert(raw(&c,(O)99,repeated[0],0,0).value.data.word==41);
 verdict=42;assert(raw(&c,(O)99,repeated[699],0,0).value.data.word==42);
 assert(sends==2); /* the symbol cache must never cache the actual dispatch */
 for(size_t i=0;i<520;i++){sprintf(names[i],"unique-%zu",i);assert(selector(&c,names[i])==(O)(1000+i));}
 assert(!c.failure.status); /* genuinely full caches still intern through Id */
 for(size_t i=520;i>0;i--)assert(selector(&c,names[i-1])==(O)(999+i));
 size_t mark=c.count;assert(literal_symbol(&c,names[519])==(O)1519);
 assert(c.count>mark); /* full-cache literal fallback still acquires protection */
 assert(!finish(&c,obj(owner)).status);
 memset(selectors,0,sizeof(selectors));
 Call failed={0};fail_owner=1;selector(&failed,"will-fail");
 assert(failed.failure.status&&failed.failure.detail==705);
 for(size_t i=0;i<BIRNAM_SELECTOR_CAPACITY;i++)assert(!selectors[i].name||strcmp(selectors[i].name,"will-fail"));
 fail_owner=0;assert(finish(&failed,word(0)).detail==705);
 Call again={0};reenter_owner=1;
 assert(selector(&again,"will-fail")== (O)4);assert(reentered);
 assert(selector(&again,"unique-519")== (O)1519);
 assert(!finish(&again,obj(owner)).status);return 0;
}
"#);
    fs::write(&source, c).unwrap();
    for capacity in [64, 128, 256, 512] {
        let output = Command::new("cc")
            .args(["-std=c11", "-include", "stdio.h"])
            .arg(format!("-DBIRNAM_SELECTOR_CAPACITY={capacity}"))
            .arg(&source)
            .arg("-o")
            .arg(&binary)
            .output()
            .unwrap();
        assert!(
            output.status.success(),
            "{}",
            String::from_utf8_lossy(&output.stderr)
        );
        let result = Command::new(&binary).output().unwrap();
        assert!(
            result.status.success(),
            "capacity {capacity}: {}",
            String::from_utf8_lossy(&result.stderr)
        );
    }
    fs::remove_dir_all(dir).unwrap();
}
