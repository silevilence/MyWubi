#include "../include/mywubi.h"
#include <assert.h>
#include <string.h>
int main(void) {
    const char *request="{\"version\":1,\"request\":{\"op\":\"Destroy\",\"handle\":0}}";
    MyWubiBuffer result = {0};
    assert(mywubi_v1_version()==1);
    assert(mywubi_v1_call((const uint8_t*)request,strlen(request),&result)==0);
    assert(result.len>0 && result.data[0]=='{');
    assert(mywubi_v1_buffer_free(result.allocation)==0);
    assert(mywubi_v1_buffer_free(result.allocation)==3);
    return 0;
}
