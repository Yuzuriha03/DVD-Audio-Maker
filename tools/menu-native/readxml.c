/* Windows XML adapter for the migrated menu callbacks. No libxml runtime.
 * XmlLite handles UTF-8, escapes and well-formedness; DTDs are prohibited. */
#include "config.h"
#include "compat.h"
#include <xmllite.h>
#include <shlwapi.h>
#include <ctype.h>
#include "readxml.h"

bool parser_err=false,parser_acceptbody=false;
char *parser_body;
static const GUID reader_iid={0x7279fc81,0x709d,0x4095,{0xb6,0x3d,0x69,0xfe,0x4b,0x0d,0x90,0x30}};
static char *utf8(const wchar_t *value,UINT length) {
    int count=WideCharToMultiByte(CP_UTF8,WC_ERR_INVALID_CHARS,value,length,NULL,0,NULL,NULL);
    if(!count && length)exit(1);
    char *s=malloc((size_t)count+1);
    if(count)WideCharToMultiByte(CP_UTF8,WC_ERR_INVALID_CHARS,value,length,s,count,NULL,NULL);
    s[count]=0;return s;
}
static char *value_of(IXmlReader *reader,int name) {
    const wchar_t *s;UINT n;HRESULT hr=name ? IXmlReader_GetQualifiedName(reader,&s,&n) : IXmlReader_GetValue(reader,&s,&n);
    if(FAILED(hr))exit(1);return utf8(s,n);
}
int readxml(const char *path,const struct elemdesc *elems,const struct elemattr *attrs) {
    IStream *stream=NULL;IXmlReader *reader=NULL;
    int history[10],state=0,root_seen=0,root_closed=0,result=1;
    int length=MultiByteToWideChar(CP_UTF8,MB_ERR_INVALID_CHARS,path,-1,NULL,0);
    if(!length)return 1;
    wchar_t *wide=malloc((size_t)length*sizeof(*wide));
    MultiByteToWideChar(CP_UTF8,MB_ERR_INVALID_CHARS,path,-1,wide,length);
    HRESULT hr=SHCreateStreamOnFileEx(wide,STGM_READ|STGM_SHARE_DENY_WRITE,0,FALSE,NULL,&stream);
    free(wide);if(FAILED(hr))goto done;
    menu_own_com((IUnknown *)stream);
    hr=CreateXmlReader(&reader_iid,(void **)&reader,NULL);
    if(FAILED(hr))goto done;
    menu_own_com((IUnknown *)reader);
    if(FAILED(IXmlReader_SetProperty(reader,XmlReaderProperty_DtdProcessing,DtdProcessing_Prohibit)) ||
       FAILED(IXmlReader_SetInput(reader,(IUnknown *)stream)))goto done;
    XmlNodeType kind;
    while((hr=IXmlReader_Read(reader,&kind))==S_OK) {
        UINT depth=0;IXmlReader_GetDepth(reader,&depth);
        /* XmlLite reports end-element depth after the open element. */
        if(kind==XmlNodeType_EndElement) { if(!depth)goto done; --depth; }
        if(depth>=10)goto done;
        if(kind==XmlNodeType_Element) {
            if(root_closed || parser_body)goto done;
            char *name=value_of(reader,1);int i;
            for(i=0;elems[i].elemname;i++)
                if(state==elems[i].parentstate && !strcmp(name,elems[i].elemname))break;
            if(!elems[i].elemname){fprintf(stderr,"ERR: unsupported menu XML element %s\n",name);free(name);goto done;}
            free(name);if(!depth)root_seen=1;
            int empty=IXmlReader_IsEmptyElement(reader);
            if(elems[i].start)elems[i].start();
            if(parser_err)goto done;
            HRESULT attr=IXmlReader_MoveToFirstAttribute(reader);
            while(attr==S_OK) {
                char *name=value_of(reader,1),*value=value_of(reader,0);int a;
                for(a=0;attrs[a].elem;a++)
                    if(!strcmp(elems[i].elemname,attrs[a].elem) && !strcmp(name,attrs[a].attr))break;
                if(!attrs[a].elem){fprintf(stderr,"ERR: unsupported menu XML attribute %s\n",name);free(name);free(value);goto done;}
                attrs[a].f(value);free(name);free(value);if(parser_err)goto done;
                attr=IXmlReader_MoveToNextAttribute(reader);
            }
            if(FAILED(attr))goto done;
            IXmlReader_MoveToElement(reader);
            if(empty) {
                if(elems[i].end)elems[i].end();
                if(parser_err)goto done;
                free(parser_body);parser_body=NULL;parser_acceptbody=false;
                if(!depth)root_closed=1;
            } else { history[depth]=i;state=elems[i].newstate; }
        } else if(kind==XmlNodeType_EndElement) {
            int i=history[depth];
            if(elems[i].end)elems[i].end();
            if(parser_err)goto done;
            state=elems[i].parentstate;free(parser_body);parser_body=NULL;parser_acceptbody=false;
            if(!depth)root_closed=1;
        } else if(kind==XmlNodeType_Text || kind==XmlNodeType_CDATA || kind==XmlNodeType_Whitespace) {
            char *text=value_of(reader,0);int only_space=1;
            for(char *c=text;*c;c++)if(!isspace((unsigned char)*c)){only_space=0;break;}
            if(!parser_acceptbody){free(text);if(!only_space)goto done;continue;}
            size_t n=parser_body ? strlen(parser_body) : 0;
            parser_body=realloc(parser_body,n+strlen(text)+1);strcpy(parser_body+n,text);free(text);
        } else if(kind!=XmlNodeType_Comment && kind!=XmlNodeType_XmlDeclaration)goto done;
    }
    if(hr==S_FALSE && root_seen && root_closed && !parser_err)result=0;
done:
    if(result)fprintf(stderr,"ERR: invalid menu XML: %s (0x%08lx, root=%d, closed=%d, state=%d, callback=%d)\n",path,(unsigned long)hr,root_seen,root_closed,state,parser_err);
    menu_release_com((IUnknown *)reader);menu_release_com((IUnknown *)stream);
    return result;
}
bool xml_ison(const char *s,const char *attr) {
    if(!strcmp(s,"1") || !strcasecmp(s,"on") || !strcasecmp(s,"yes"))return true;
    if(!strcmp(s,"0") || !strcasecmp(s,"off") || !strcasecmp(s,"no"))return false;
    fprintf(stderr,"ERR: invalid boolean for %s\n",attr);exit(1);
}
