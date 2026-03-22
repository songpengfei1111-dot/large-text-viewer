import idautils
import idaapi
import idc

def get_all_named_functions():
    """
    获取所有有符号函数的地址和名称
    返回: 一个列表，包含元组 (地址, 函数名)
    """
    named_functions = []
    
    # idautils.Functions() 返回所有函数的起始地址列表
    for func_ea in idautils.Functions():
        # 获取函数名称 (如果函数有名称，idc.get_func_name 会返回它)
        func_name = idc.get_func_name(func_ea)
        
        # 检查是否为有符号函数 (排除没有名字的 sub_ 或 nullsub 等自动生成的名称)
        # 通常，有意义的函数名不包含 'sub_' 前缀
        if func_name and not func_name.startswith('sub_'):
            named_functions.append((func_ea, func_name))
            # 可以选择打印或处理
            print(f"0x{func_ea:X} : {func_name}")
            
    return named_functions

# 调用函数
all_functions = get_all_named_functions()
print(f"总共找到 {len(all_functions)} 个有符号函数")