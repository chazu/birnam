//! Native command records projected through ordinary object messages.
//! No transport text, shared record storage, or new runtime object family.
use crate::{project::*, HostError, Scope};
use birnam_abi::{self as abi, Oop};
use std::{ffi::OsString, os::unix::ffi::{OsStrExt, OsStringExt}, path::PathBuf};

pub trait CommandValue: Sized {
    fn to_object(&self, q: &Scope<'_>) -> Result<Oop, HostError>;
    fn from_object(q: &Scope<'_>, value: Oop) -> Result<Self, HostError>;
}
impl CommandValue for String {
    fn to_object(&self, q: &Scope<'_>) -> Result<Oop, HostError> { q.string(self.as_bytes()) }
    fn from_object(q: &Scope<'_>, v: Oop) -> Result<Self, HostError> {
        String::from_utf8(q.string_bytes(v)?).map_err(|e| HostError(e.to_string()))
    }
}
impl CommandValue for Vec<u8> {
    fn to_object(&self, q: &Scope<'_>) -> Result<Oop, HostError> {
        abi::byte_transfer::buffer(self, q.host.named(b"Bytes")?, |r,n,a| q.host.send(r,n,a), |v|q.keep(v))
    }
    fn from_object(q: &Scope<'_>, v: Oop) -> Result<Self, HostError> { q.read_bytes(v) }
}
impl CommandValue for OsString {
    fn to_object(&self, q: &Scope<'_>) -> Result<Oop, HostError> { self.as_bytes().to_vec().to_object(q) }
    fn from_object(q: &Scope<'_>, v: Oop) -> Result<Self, HostError> { Ok(Self::from_vec(Vec::<u8>::from_object(q,v)?)) }
}
impl CommandValue for PathBuf {
    fn to_object(&self, q: &Scope<'_>) -> Result<Oop, HostError> { self.as_os_str().to_owned().to_object(q) }
    fn from_object(q: &Scope<'_>, v: Oop) -> Result<Self, HostError> { Ok(OsString::from_object(q,v)?.into()) }
}
impl CommandValue for bool {
    fn to_object(&self, q: &Scope<'_>) -> Result<Oop, HostError> { q.host.named(if *self { b"true" } else { b"false" }) }
    fn from_object(q: &Scope<'_>, v: Oop) -> Result<Self, HostError> {
        if v == q.host.named(b"true")? { Ok(true) } else if v == q.host.named(b"false")? { Ok(false) } else { Err(HostError("expected Boolean".into())) }
    }
}
impl CommandValue for i64 {
    fn to_object(&self, q: &Scope<'_>) -> Result<Oop, HostError> {
        q.call(q.host.named(b"Integer")?, b"fromWord:", &[q.word(*self as usize)?])
    }
    fn from_object(q: &Scope<'_>, v: Oop) -> Result<Self, HostError> {
        q.exact(v,b"IntegerVt")?;
        Ok(q.word_bits(q.call(q.host.named(b"Integer")?, b"wordOf:", &[v])?)? as i64)
    }
}
macro_rules! count {
    ($t:ty) => { impl CommandValue for $t {
        fn to_object(&self, q: &Scope<'_>) -> Result<Oop, HostError> {
            i64::try_from(*self).map_err(|_|HostError("command count exceeds Integer range".into()))?.to_object(q)
        }
        fn from_object(q: &Scope<'_>, v: Oop) -> Result<Self, HostError> {
            Self::try_from(i64::from_object(q,v)?).map_err(|_|HostError("invalid command count".into()))
        }
    }};
}
count!(usize); count!(u16);
impl<T: CommandValue> CommandValue for Option<T> {
    fn to_object(&self, q: &Scope<'_>) -> Result<Oop, HostError> { match self { Some(v)=>v.to_object(q), None=>q.host.named(b"nil") } }
    fn from_object(q: &Scope<'_>, v: Oop) -> Result<Self, HostError> { if v == q.host.named(b"nil")? { Ok(None) } else { T::from_object(q,v).map(Some) } }
}
impl<T: CommandValue> CommandValue for Vec<T> {
    fn to_object(&self, q: &Scope<'_>) -> Result<Oop, HostError> {
        let local=q.host.scope();
        let mut result=local.call(q.host.named(b"Vector")?,b"new",&[])?;
        for value in self {
            let item=q.host.scope(); let value=value.to_object(&item)?;
            result=local.replace_kept(result,q.host.send(result,b"add:",&[value])?)?;
        }
        q.keep(result)
    }
    fn from_object(q: &Scope<'_>, v: Oop) -> Result<Self, HostError> {
        (0..q.vector_size(v)?).map(|i| { let item=q.host.scope(); T::from_object(&item,item.vector_at(v,i)?) }).collect()
    }
}
impl<A:CommandValue,B:CommandValue> CommandValue for (A,B) {
    fn to_object(&self,q:&Scope<'_>)->Result<Oop,HostError>{
        let local=q.host.scope();let a=self.0.to_object(&local)?;let b=self.1.to_object(&local)?;
        let v=local.call(q.host.named(b"Vector")?,b"new",&[])?;let v=local.call(v,b"add:",&[a])?;
        q.keep(q.host.send(v,b"add:",&[b])?)
    }
    fn from_object(q:&Scope<'_>,v:Oop)->Result<Self,HostError>{
        if q.vector_size(v)?!=2{return Err(HostError("expected pair".into()));}
        Ok((A::from_object(q,q.vector_at(v,0)?)?,B::from_object(q,q.vector_at(v,1)?)?))
    }
}
impl CommandValue for ReplMode {
    fn to_object(&self,q:&Scope<'_>)->Result<Oop,HostError>{
        match self { Self::None=>"None", Self::Tty=>"Tty",Self::Socket=>"Socket" }.to_owned().to_object(q)
    }
    fn from_object(q:&Scope<'_>,v:Oop)->Result<Self,HostError>{
        match String::from_object(q,v)?.as_str(){"None"=>Ok(Self::None),"Tty"=>Ok(Self::Tty),"Socket"=>Ok(Self::Socket),_=>Err(HostError("invalid REPL mode".into()))}
    }
}
// Field declarations keep command records explicit. These are ordinary Maps;
// the macro only avoids duplicating the encode/decode field names in Rust.
macro_rules! record {
    ($t:ty { $($field:ident),* $(,)? }) => {impl CommandValue for $t {
        fn to_object(&self,q:&Scope<'_>)->Result<Oop,HostError>{
            let local=q.host.scope();let mut value=local.call(q.host.named(b"Map")?,b"new",&[])?;
            $(let field=q.host.scope(); let key=field.string(stringify!($field).as_bytes())?;
              let item=self.$field.to_object(&field)?;
              value=local.replace_kept(value,q.host.send(value,b"at:put:",&[key,item])?)?;)*
            q.keep(value)
        }
        fn from_object(q:&Scope<'_>,value:Oop)->Result<Self,HostError>{
            q.exact(value,b"MapVt")?;
            Ok(Self { $($field: { let field=q.host.scope();let key=field.string(stringify!($field).as_bytes())?;
                CommandValue::from_object(&field,field.call(value,b"at:",&[key])?)? }),* })
        }
    }};
}
record!(RunOptions { directory, build, profile, repl, socket_bind, socket_port, arguments });
record!(ProjectRunObservation { repl, socket_bind, socket_port });
record!(ProjectWrappedArtifactObservation { project_root, name, build, packaging });
record!(ProjectFetchResult { project_root, fetched, reused });
record!(ProjectResolutionObservation { project_root, manifest, project_name, main, run, load_roots, modules, wrapped });
record!(ProjectCompileDiagnostic { module, path, detail, message, line, column });
record!(ProjectCheckResult { resolution, resolution_digest, wrapped, test_sources, diagnostics });
record!(ProjectStagedArtifact { name, path, action, checksum });
record!(ProjectBuildResult { output_root, module_root, configuration, application, artifacts });
record!(ProjectSourceMetadata { name, requires, source_digest, revision_digest });
record!(ProjectSourceObservation { name, path, sources, project_root, source, requires, source_digest, revision_digest });
