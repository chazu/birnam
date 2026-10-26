#include "id.h"
Oop check_oop_id(const IdProtocol *p,Oop actor,Oop dynamic,Oop gc,Oop collect,Oop roots,Oop retain,Oop release){
 Oop arg=id_size(64);
 Oop name=p->message(p->root,actor,dynamic,&arg,1);if(id_fault(name))return name;
 Oop token=p->message(p->root,roots,retain,&name,1);if(id_fault(token))return token;
 Oop symbol=p->message(p->root,name,p->intern,0,0);if(id_fault(symbol))return symbol;
 Oop token2=p->message(p->root,roots,retain,&symbol,1);if(id_fault(token2))return token2;
 Oop result=p->message(p->root,gc,collect,0,0);if(id_fault(result))return result;
 Oop again=p->message(p->root,name,p->intern,0,0);if(again!=symbol)return p->invalid;
 result=p->message(p->root,roots,release,&token2,1);if(id_fault(result))return result;
 result=p->message(p->root,roots,release,&token,1);if(id_fault(result))return result;
 return symbol;
}
