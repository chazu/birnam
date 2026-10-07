//! Execute the production prelude: order, refusal, failure precedence and roots.
use std::{fs, process::Command};
#[test]
fn generated_cleanup_batches_or_uses_ordered_live_sends_and_leaks_no_temporary_roots() {
    let dir = std::env::temp_dir().join(format!("birnam-root-cleanup-{}", std::process::id()));
    fs::create_dir_all(&dir).unwrap();
    let source = dir.join("test.c");
    let binary = dir.join("test");
    let mut c = include_str!("../src/native_prelude.c").to_string();
    c.push_str(r#"
#include <assert.h>
static size_t next_token,live[256],order[256],released,batches,individual;
static int mode;
static R message(void*ctx,O r,O sel,const A*a,size_t n){
 (void)ctx; (void)n;
 if(sel==id.intern){size_t len=a[0].data.bytes.length;char*p=malloc(len+1);memcpy(p,a[0].data.bytes.pointer,len);p[len]=0;return ok(obj(p));}
 const char*name=(const char*)sel;
 if(r==owner)return ok(obj(owner));
 assert(r==roots);
 if(!strcmp(name,"retain:")){live[++next_token]=1;return ok(word(next_token));}
 if(!strcmp(name,"tryReleaseMany:")){
  batches++; if(mode==1)return ok(word(0));if(mode==2){R e=bad();e.detail=902;return e;}
  const size_t*t=(const size_t*)a[0].data.bytes.pointer;size_t count=a[0].data.bytes.length/sizeof(size_t);
  for(size_t i=count;i>0;i--){assert(live[t[i-1]]);live[t[i-1]]=0;order[released++]=t[i-1];}return ok(word(1));
 }
 assert(!strcmp(name,"release:"));individual++;size_t t=a[0].data.word;
 assert(live[t]);live[t]=0;order[released++]=t;return ok(word(1));
}
int main(void){
 id.message=message;id.receiver=(O)10;id.intern=(O)11;roots=(O)12;owner=(O)13;cache_selectors=1;
 for(mode=0;mode<3;mode++){
  size_t previous=next_token;released=batches=individual=0;Call c={0};
  for(size_t i=0;i<16;i++)keep(&c,(O)(100+i));
  if(mode<2){A input[3]={obj((O)201),obj((O)202),obj((O)203)};size_t old=next_token;check_entry(&c,(O)200,input,3);assert(next_token==old);keep(&c,(O)201);assert(next_token==old);c.input_count=0;keep_entry(&c,(O)200,input,3);assert(next_token>old);}
  if(mode==2){c.failure=bad();c.failure.detail=901;}
  R result=finish(&c,word(77));assert(batches==1);
  if(mode==2){assert(result.status&&result.detail==901);}else assert(!result.status&&result.value.data.word==77);
  assert(released==next_token-previous);
  for(size_t i=0;i<released;i++)assert(order[i]==next_token-i);
  for(size_t i=previous+1;i<=next_token;i++)assert(!live[i]);
  assert(individual==(mode?released:0));
 }
 return 0;
}
"#);
    fs::write(&source, c).unwrap();
    let output = Command::new("cc")
        .args(["-std=c11"])
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
        "{}",
        String::from_utf8_lossy(&result.stderr)
    );
    fs::remove_dir_all(dir).unwrap();
}
