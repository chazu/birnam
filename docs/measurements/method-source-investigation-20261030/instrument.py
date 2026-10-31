from pathlib import Path
import sys
r=Path(sys.argv[1]).resolve()
helper='''// Investigation-only counters; not part of the language or shipped ABI.
use std::sync::atomic::{AtomicU64,AtomicUsize,Ordering::Relaxed};
static COUNTS:[AtomicU64;16]=[const {AtomicU64::new(0)};16];
static SELECTORS:[AtomicUsize;32]=[const {AtomicUsize::new(0)};32];
static SENDS:[AtomicU64;32]=[const {AtomicU64::new(0)};32];
pub fn inc(i:usize){COUNTS[i].fetch_add(1,Relaxed);}
pub fn add(i:usize,n:u64){COUNTS[i].fetch_add(n,Relaxed);}
pub fn message(selector:usize){for i in 0..32 {if SELECTORS[i].load(Relaxed)==selector {SENDS[i].fetch_add(1,Relaxed);}}}
pub struct Timer(std::time::Instant);
impl Timer {pub fn new()->Self{inc(0);Self(std::time::Instant::now())}}
impl Drop for Timer {fn drop(&mut self){add(1,self.0.elapsed().as_nanos() as u64);}}
#[unsafe(no_mangle)]
pub extern "C" fn EXPORT(op:usize,index:usize,value:usize)->u64{match op {
 0=>COUNTS[index].load(Relaxed),
 1=>{for c in &COUNTS {c.store(0,Relaxed);}for c in &SENDS{c.store(0,Relaxed);}0},
 2=>{SELECTORS[index].store(value,Relaxed);0},
 3=>SENDS[index].load(Relaxed),_=>0}}
'''
for name in ['id','root-registry','collect']:
 d=r/f'rust/birnam-{name}/src';(d/'probe.rs').write_text(helper.replace('EXPORT','birnam_'+name.replace('-','_')+'_probe'))
 p=d/'lib.rs';s=p.read_text();lines=s.splitlines(True);i=0
 while lines[i].startswith('//!'):i+=1
 lines.insert(i,'#[allow(dead_code)] mod probe;\n');p.write_text(''.join(lines))
def edit(path,old,new):
 p=r/path;s=p.read_text();assert s.count(old)==1,(path,old,s.count(old));p.write_text(s.replace(old,new))
base='rust/birnam-id/src/id/'
edit(base+'id_dispatch.rs','    let behavior = s.behavior(r)?;','    crate::probe::inc(1);\n    let behavior = s.behavior(r)?;')
edit(base+'id_dispatch.rs','    let r=crate::abi::object_reference(r);','    crate::probe::inc(0);crate::probe::message(selector.addr());\n    let r=crate::abi::object_reference(r);')
edit(base+'native_id.rs','    pub fn behavior(&self, o: Oop) -> Result<Oop, Oop> {','    pub fn behavior(&self, o: Oop) -> Result<Oop, Oop> {\n        crate::probe::inc(2);')
edit(base+'native_id.rs','    pub(super) fn payload(&self, o: Oop) -> Result<(*mut u8, usize), Oop> {','    pub(super) fn payload(&self, o: Oop) -> Result<(*mut u8, usize), Oop> {\n        crate::probe::inc(3);')
edit(base+'native_id.rs','        let mut frames = self.active.borrow_mut();','        crate::probe::inc(6);\n        let mut frames = self.active.borrow_mut();')
edit(base+'bootstrap_values.rs','    pub(super) fn method_data(&self, o: Oop) -> Result<MethodData, Oop> {','    pub(super) fn method_data(&self, o: Oop) -> Result<MethodData, Oop> {\n        crate::probe::inc(4);')
edit(base+'bootstrap_values.rs','        let method = self.method_data(m)?;','        crate::probe::inc(5);\n        let method = self.method_data(m)?;')
edit(base+'object_memory.rs','    pub(crate) fn get(&self, address: usize) -> Option<&Allocation> {','    pub(crate) fn get(&self, address: usize) -> Option<&Allocation> {\n        crate::probe::inc(7);')
edit(base+'object_memory.rs','    pub(crate) fn allocate(&mut self, payload_size: usize, vtable: Oop) -> Oop {','    pub(crate) fn allocate(&mut self, payload_size: usize, vtable: Oop) -> Oop {\n        crate::probe::inc(8);crate::probe::add(9,payload_size as u64);')
base='rust/birnam-root-registry/src/lib.rs'
edit(base,'    fn retain(&mut self, o: Oop) -> Option<usize> {','    fn retain(&mut self, o: Oop) -> Option<usize> {\n        crate::probe::inc(0);')
edit(base,'    fn release(&mut self, t: usize) -> bool {','    fn release(&mut self, t: usize) -> bool {\n        crate::probe::inc(1);')
edit(base,'unsafe extern "C" fn release_many(_: Oop, c: Oop, _: Oop, a: *const Oop, n: usize) -> Oop {','unsafe extern "C" fn release_many(_: Oop, c: Oop, _: Oop, a: *const Oop, n: usize) -> Oop {\n    crate::probe::inc(2);crate::probe::add(3,n.saturating_sub(1) as u64);')
base='rust/birnam-collect/src/collection.rs'
edit(base,'    s.phase.set(Phase::Marking);\n    let result = match unsafe { mark(s, native) } {','    let _probe_timer = crate::probe::Timer::new();\n    s.phase.set(Phase::Marking);\n    let result = match unsafe { mark(s, native) } {')
edit('rust/birnam-id/src/id/continuation.rs','    fn work(&self, entry: usize, values: &[Oop]) -> Result<Oop, Oop> {','    fn work(&self, entry: usize, values: &[Oop]) -> Result<Oop, Oop> {\n        crate::probe::inc(11);')
edit('rust/birnam-id/src/id/continuation.rs','        // A heap context, not a pointer to a native stack result slot.','        crate::probe::inc(10);\n        // A heap context, not a pointer to a native stack result slot.')
edit('rust/birnam-id/src/id/bootstrap_values.rs','    pub(super) fn new_sequence(&self, values: &[Oop]) -> Result<Oop, Oop> {','    pub(super) fn new_sequence(&self, values: &[Oop]) -> Result<Oop, Oop> {\n        crate::probe::inc(12);')
edit('rust/birnam-id/src/id/bootstrap_values.rs','    fn method_with_behavior(&self, entry_index: usize, context: Oop, owner: Oop, behavior: Oop) -> Result<Oop, Oop> {','    fn method_with_behavior(&self, entry_index: usize, context: Oop, owner: Oop, behavior: Oop) -> Result<Oop, Oop> {\n        crate::probe::inc(13);')
print('Instrumented isolated source.')
