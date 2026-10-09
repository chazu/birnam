//! A root-free final send still dispatches live; nonempty/cold paths clean up.
use std::{fs, process::Command};
#[test]
fn final_result_handoff_and_literal_coverage_have_real_fallbacks() {
    let dir = std::env::temp_dir().join(format!("birnam-return-handoff-{}", std::process::id()));
    fs::create_dir_all(&dir).unwrap();
    let source = dir.join("handoff.c"); let binary = dir.join("handoff");
    let mut c = include_str!("../src/native_prelude.c").to_owned();
    c.push_str(r#"
#include <assert.h>
static size_t retained,released,calls,next;static O live[128];static int mode;
static R message(void*ctx,O receiver,O sel,const A*a,size_t n){
 (void)ctx;(void)n;
 if(sel==id.intern)return ok(obj((O)a[0].data.bytes.pointer));
 const char*name=(const char*)sel;
 if(receiver==roots){
  if(!strcmp(name,"retain:")){retained++;live[++next]=a[0].data.object;return ok(word(next));}
  if(!strcmp(name,"releaseMany:protecting:batch:")){
   assert(n==3&&a[1].kind==1);size_t count=a[0].data.bytes.length/sizeof(size_t);
   const size_t*tokens=(const size_t*)a[0].data.bytes.pointer;
   for(size_t i=count;i>0;i--){size_t t=tokens[i-1];assert(live[t]);live[t]=0;released++;}
   return ok(word(1));
  }
  assert(!strcmp(name,"release:"));size_t t=a[0].data.word;assert(live[t]);live[t]=0;released++;if(mode==5){R r=bad();r.status=914;return r;}return ok(word(1));
 }
 if(receiver==owner)return ok(obj(owner));
 calls++;
 if(mode==1){R r=bad();r.status=812;r.detail=19;return r;}
 if(mode==2)return ok(word(7));
 if(mode==3)return ok(obj(0));
 if(mode==4){int found=0;for(size_t i=1;i<=next;i++)if(live[i]==(O)55)found=1;assert(found);}
 return ok(obj((O)777));
}
int main(void){
 id.message=message;id.receiver=(O)10;id.intern=(O)11;roots=(O)12;owner=(O)13;cache_selectors=1;batch_cleanup=0;
 Call warm={0};selector(&warm,"target");literal_symbol(&warm,"literal");finish(&warm,obj(owner));
 size_t before=retained,cleanup=released;
 for(mode=0;mode<4;mode++){
  Call c={0};check_entry(&c,(O)99,0,0);R r=return_call(&c,(O)99,"target",0,0);
  if(!mode)assert(!r.status&&r.value.data.object==(O)777);
  else if(mode==1)assert(r.status==812&&r.detail==19);
  else assert(r.status==1);
 }
 assert(retained==before&&released==cleanup&&calls==4);
 Call literal={0};literal.reuse=1;assert(literal_symbol(&literal,"literal"));assert(!literal.count);finish(&literal,obj(owner));
 mode=4;Call locals={0};locals.reuse=1;keep(&locals,(O)55);R r=return_call(&locals,(O)99,"target",0,0);
 assert(!r.status&&r.value.data.object==(O)777);assert(retained==before+2&&released==cleanup+2);
 mode=0;Call cold={0};cold.reuse=1;r=return_call(&cold,(O)99,"new-target",0,0);assert(!r.status);assert(retained>before+2);
 /* A genuinely full cache cannot promise that an interned literal is owned. */
 for(size_t i=0;i<512;i++)selectors[i]=(Selector){"occupied",(O)99};
 Call full={0};full.reuse=1;assert(literal_symbol(&full,"uncached"));assert(full.count==1);finish(&full,obj(owner));
 /* Body failure wins; later cleanup failures still drain every token. */
 mode=5;Call failed={0};keep(&failed,(O)55);keep(&failed,(O)66);
 size_t before_errors=released;failed.failure=bad();failed.failure.status=812;failed.failure.detail=19;
 r=finish(&failed,obj((O)777));assert(r.status==812&&r.detail==19);assert(released==before_errors+2);
 return 0;
}
"#);
    fs::write(&source, c).unwrap();
    let output = Command::new("cc").args(["-std=c11"]).arg(&source).arg("-o").arg(&binary).output().unwrap();
    assert!(output.status.success(), "{}", String::from_utf8_lossy(&output.stderr));
    let result = Command::new(&binary).output().unwrap();
    assert!(result.status.success(), "{}", String::from_utf8_lossy(&result.stderr));
    fs::remove_dir_all(dir).unwrap();
}
