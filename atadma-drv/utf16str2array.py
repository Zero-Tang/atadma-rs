import codecs
import sys

if __name__=="__main__":
    s=codecs.encode(sys.argv[1].encode(),"hex").decode()
    L=[]
    for i in range(0,len(s),2):
        L.append("0x"+s[i:i+2])
    ss="let s:[u16;{}]=[".format(len(L)+1)
    for x in L:
        ss+=x+","
    ss+="0];"
    print(ss)